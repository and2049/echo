//! A single-line text field: caret and selection, the usual editing shortcuts, clipboard, mouse
//! selection, and input-method composition (pinyin, kana, accent hold) through the platform
//! input handler.
//!
//! Typed text never comes from `KeyDownEvent::key_char`: the field registers an input handler
//! while it paints, and the platform delivers text (and composition updates) through
//! [`EntityInputHandler`]. Enter, Escape, Tab and the arrow keys a field doesn't bind fall
//! through to the owner's `on_key_down`, so each owner decides what submitting means.

mod buffer;

use std::ops::Range;

use buffer::TextBuffer;
use gpui::{
    App, Bounds, ClipboardItem, ContentMask, Context, CursorStyle, ElementId, ElementInputHandler,
    Entity, EntityInputHandler, EventEmitter, FocusHandle, GlobalElementId, Hsla, KeyBinding,
    LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point,
    ShapedLine, SharedString, Style, TextRun, UTF16Selection, UnderlineStyle, Window, actions, div,
    fill, point, prelude::*, px, relative, size,
};

const CONTEXT: &str = "TextField";

actions!(
    text_field,
    [
        Backspace,
        Delete,
        DeleteWordLeft,
        DeleteWordRight,
        DeleteToLineStart,
        DeleteToLineEnd,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        WordLeft,
        WordRight,
        SelectWordLeft,
        SelectWordRight,
        LineStart,
        LineEnd,
        SelectToLineStart,
        SelectToLineEnd,
        SelectAll,
        Copy,
        Cut,
        Paste,
        ShowCharacterPalette,
    ]
);

/// The editing keymap, scoped to a focused field so it outranks the list and global bindings
/// (alt-left is history-back elsewhere, word-left here). Each platform gets its own
/// conventions: cmd and alt on macOS, ctrl elsewhere; ctrl-backspace deletes a word on all.
pub(crate) fn bind_keys(cx: &mut App) {
    let ctx = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, ctx),
        KeyBinding::new("shift-backspace", Backspace, ctx),
        KeyBinding::new("delete", Delete, ctx),
        KeyBinding::new("ctrl-backspace", DeleteWordLeft, ctx),
        KeyBinding::new("ctrl-delete", DeleteWordRight, ctx),
        KeyBinding::new("left", Left, ctx),
        KeyBinding::new("right", Right, ctx),
        KeyBinding::new("shift-left", SelectLeft, ctx),
        KeyBinding::new("shift-right", SelectRight, ctx),
        KeyBinding::new("home", LineStart, ctx),
        KeyBinding::new("end", LineEnd, ctx),
        KeyBinding::new("shift-home", SelectToLineStart, ctx),
        KeyBinding::new("shift-end", SelectToLineEnd, ctx),
    ]);
    if cfg!(target_os = "macos") {
        cx.bind_keys([
            KeyBinding::new("alt-backspace", DeleteWordLeft, ctx),
            KeyBinding::new("alt-delete", DeleteWordRight, ctx),
            KeyBinding::new("cmd-backspace", DeleteToLineStart, ctx),
            KeyBinding::new("cmd-delete", DeleteToLineEnd, ctx),
            KeyBinding::new("alt-left", WordLeft, ctx),
            KeyBinding::new("alt-right", WordRight, ctx),
            KeyBinding::new("alt-shift-left", SelectWordLeft, ctx),
            KeyBinding::new("alt-shift-right", SelectWordRight, ctx),
            KeyBinding::new("cmd-left", LineStart, ctx),
            KeyBinding::new("cmd-right", LineEnd, ctx),
            KeyBinding::new("cmd-shift-left", SelectToLineStart, ctx),
            KeyBinding::new("cmd-shift-right", SelectToLineEnd, ctx),
            // The Cocoa text-system defaults every native field honours.
            KeyBinding::new("ctrl-a", LineStart, ctx),
            KeyBinding::new("ctrl-e", LineEnd, ctx),
            KeyBinding::new("cmd-a", SelectAll, ctx),
            KeyBinding::new("cmd-c", Copy, ctx),
            KeyBinding::new("cmd-x", Cut, ctx),
            KeyBinding::new("cmd-v", Paste, ctx),
            KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, ctx),
        ]);
    } else {
        cx.bind_keys([
            KeyBinding::new("ctrl-left", WordLeft, ctx),
            KeyBinding::new("ctrl-right", WordRight, ctx),
            KeyBinding::new("ctrl-shift-left", SelectWordLeft, ctx),
            KeyBinding::new("ctrl-shift-right", SelectWordRight, ctx),
            KeyBinding::new("ctrl-a", SelectAll, ctx),
            KeyBinding::new("ctrl-c", Copy, ctx),
            KeyBinding::new("ctrl-x", Cut, ctx),
            KeyBinding::new("ctrl-v", Paste, ctx),
            KeyBinding::new("shift-insert", Paste, ctx),
        ]);
    }
}

/// What a field does to text before it goes in, typed or pasted.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum InputFilter {
    /// Line breaks are dropped, so a copied line cannot smuggle one into a query.
    DropNewlines,
}

impl InputFilter {
    fn apply(self, text: &str) -> String {
        match self {
            Self::DropNewlines => text.chars().filter(|c| *c != '\r' && *c != '\n').collect(),
        }
    }
}

/// The colours a field paints itself; text takes the inherited text style.
#[derive(Clone, Copy)]
pub(crate) struct TextFieldColors {
    pub(crate) placeholder: Hsla,
    pub(crate) caret: Hsla,
    pub(crate) selection: Hsla,
}

pub(crate) enum TextFieldEvent {
    /// The text changed, by typing, pasting, deleting or a composition update.
    Changed,
}

/// Whether a mouse drag is extending the selection.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pointer {
    Idle,
    Selecting,
}

pub(crate) struct TextField {
    buffer: TextBuffer,
    focus_handle: FocusHandle,
    filter: InputFilter,
    placeholder: SharedString,
    colors: TextFieldColors,
    pointer: Pointer,
    /// The last painted line and its bounds, for pointer hit-testing and placing the input
    /// method's candidate window.
    layout: Option<ShapedLine>,
    bounds: Option<Bounds<Pixels>>,
    /// How far the text is scrolled left so the caret stays in view.
    scroll_x: Pixels,
}

impl EventEmitter<TextFieldEvent> for TextField {}

impl TextField {
    pub(crate) fn new(focus_handle: FocusHandle, filter: InputFilter) -> Self {
        Self {
            buffer: TextBuffer::default(),
            focus_handle,
            filter,
            placeholder: SharedString::default(),
            colors: TextFieldColors {
                placeholder: gpui::transparent_black(),
                caret: gpui::transparent_black(),
                selection: gpui::transparent_black(),
            },
            pointer: Pointer::Idle,
            layout: None,
            bounds: None,
            scroll_x: px(0.0),
        }
    }

    pub(crate) fn text(&self) -> &str {
        self.buffer.text()
    }

    /// Whether an input method is mid-composition; owners leave Enter to it then.
    pub(crate) fn is_composing(&self) -> bool {
        self.buffer.marked().is_some()
    }

    /// Replaces the text, caret at the end. Neither emits `Changed` nor notifies: the owner made
    /// the change and redraws itself, and with it the field.
    pub(crate) fn set_text(&mut self, text: &str) {
        self.buffer.set_text(text);
    }

    /// Theme- and language-dependent settings, refreshed by the owner each render.
    pub(crate) fn configure(&mut self, placeholder: SharedString, colors: TextFieldColors) {
        self.placeholder = placeholder;
        self.colors = colors;
    }

    fn edited(&mut self, cx: &mut Context<Self>) {
        cx.emit(TextFieldEvent::Changed);
        cx.notify();
    }

    fn delete_to(&mut self, to: usize, cx: &mut Context<Self>) {
        if self.buffer.delete_to(to) {
            self.edited(cx);
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.buffer.move_to(offset);
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.buffer.select_to(offset);
        cx.notify();
    }

    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        self.delete_to(self.buffer.prev_grapheme(self.buffer.cursor()), cx);
    }

    fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        self.delete_to(self.buffer.next_grapheme(self.buffer.cursor()), cx);
    }

    fn delete_word_left(&mut self, _: &DeleteWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.delete_to(self.buffer.prev_word(self.buffer.cursor()), cx);
    }

    fn delete_word_right(&mut self, _: &DeleteWordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.delete_to(self.buffer.next_word(self.buffer.cursor()), cx);
    }

    fn delete_to_line_start(
        &mut self,
        _: &DeleteToLineStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.delete_to(0, cx);
    }

    fn delete_to_line_end(&mut self, _: &DeleteToLineEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.delete_to(self.buffer.text().len(), cx);
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.move_left();
        cx.notify();
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.move_right();
        cx.notify();
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.buffer.prev_grapheme(self.buffer.cursor()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.buffer.next_grapheme(self.buffer.cursor()), cx);
    }

    fn word_left(&mut self, _: &WordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.buffer.prev_word(self.buffer.cursor()), cx);
    }

    fn word_right(&mut self, _: &WordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.buffer.next_word(self.buffer.cursor()), cx);
    }

    fn select_word_left(&mut self, _: &SelectWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.buffer.prev_word(self.buffer.cursor()), cx);
    }

    fn select_word_right(&mut self, _: &SelectWordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.buffer.next_word(self.buffer.cursor()), cx);
    }

    fn line_start(&mut self, _: &LineStart, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn line_end(&mut self, _: &LineEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.buffer.text().len(), cx);
    }

    fn select_to_line_start(
        &mut self,
        _: &SelectToLineStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_to(0, cx);
    }

    fn select_to_line_end(&mut self, _: &SelectToLineEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.buffer.text().len(), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.select_all();
        cx.notify();
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        let selected = self.buffer.selected_text();
        if !selected.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(selected.to_string()));
        }
    }

    fn cut(&mut self, _: &Cut, _: &mut Window, cx: &mut Context<Self>) {
        let selected = self.buffer.selected_text();
        if !selected.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(selected.to_string()));
            self.buffer.replace(None, "");
            self.edited(cx);
        }
    }

    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            let text = self.filter.apply(&text);
            self.buffer.replace(None, &text);
            self.edited(cx);
        }
    }

    fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle, cx);
        let index = self.index_for_position(event.position);
        match event.click_count {
            1 if event.modifiers.shift => self.buffer.select_to(index),
            1 => self.buffer.move_to(index),
            2 => self.buffer.select_word_at(index),
            _ => self.buffer.select_all(),
        }
        self.pointer = Pointer::Selecting;
        cx.notify();
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.pointer = Pointer::Idle;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.pointer == Pointer::Selecting && event.pressed_button == Some(MouseButton::Left) {
            self.select_to(self.index_for_position(event.position), cx);
        }
    }

    fn index_for_position(&self, position: Point<Pixels>) -> usize {
        let (Some(bounds), Some(line)) = (self.bounds, self.layout.as_ref()) else {
            return 0;
        };
        if self.buffer.text().is_empty() {
            return 0;
        }
        line.closest_index_for_x(position.x - bounds.left() + self.scroll_x)
    }
}

impl EntityInputHandler for TextField {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.buffer.range_from_utf16(&range_utf16);
        actual_range.replace(self.buffer.range_to_utf16(&range));
        Some(self.buffer.text()[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.buffer.range_to_utf16(&self.buffer.selection()),
            reversed: self.buffer.reversed(),
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.buffer
            .marked()
            .map(|range| self.buffer.range_to_utf16(&range))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.unmark();
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16.map(|range| self.buffer.range_from_utf16(&range));
        let text = self.filter.apply(text);
        self.buffer.replace(range, &text);
        self.edited(cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16.map(|range| self.buffer.range_from_utf16(&range));
        let text = self.filter.apply(text);
        // The selection the input method reports is in UTF-16 units of the marked text alone.
        let selected = new_selected_range_utf16.map(|selected| {
            let mut composing = TextBuffer::default();
            composing.set_text(&text);
            composing.range_from_utf16(&selected)
        });
        self.buffer.replace_and_mark(range, &text, selected);
        self.edited(cx);
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let layout = self.layout.as_ref()?;
        let range = self.buffer.range_from_utf16(&range_utf16);
        let left = bounds.left() - self.scroll_x;
        Some(Bounds::from_corners(
            point(left + layout.x_for_index(range.start), bounds.top()),
            point(left + layout.x_for_index(range.end), bounds.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let bounds = self.bounds?;
        let layout = self.layout.as_ref()?;
        let index = layout.index_for_x(point.x - bounds.left() + self.scroll_x)?;
        Some(self.buffer.text_to_utf16(index))
    }
}

impl Render for TextField {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::delete_word_left))
            .on_action(cx.listener(Self::delete_word_right))
            .on_action(cx.listener(Self::delete_to_line_start))
            .on_action(cx.listener(Self::delete_to_line_end))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::word_left))
            .on_action(cx.listener(Self::word_right))
            .on_action(cx.listener(Self::select_word_left))
            .on_action(cx.listener(Self::select_word_right))
            .on_action(cx.listener(Self::line_start))
            .on_action(cx.listener(Self::line_end))
            .on_action(cx.listener(Self::select_to_line_start))
            .on_action(cx.listener(Self::select_to_line_end))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::show_character_palette))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .w_full()
            .min_w_0()
            .child(TextElement { field: cx.entity() })
    }
}

/// Paints the field's line: selection, text (composition underlined), caret; and registers
/// the field as the window's input handler while it is focused.
struct TextElement {
    field: Entity<TextField>,
}

struct Prepainted {
    line: ShapedLine,
    selection: Option<PaintQuad>,
    caret: Option<PaintQuad>,
    scroll_x: Pixels,
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = Prepainted;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let field = self.field.read(cx);
        let text = field.buffer.text();
        let selection = field.buffer.selection();
        let cursor = field.buffer.cursor();
        let style = window.text_style();

        let (display, color) = if text.is_empty() {
            (field.placeholder.clone(), field.colors.placeholder)
        } else {
            (SharedString::from(text.to_string()), style.color)
        };
        let run = TextRun {
            len: display.len(),
            font: style.font(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = match field.buffer.marked().filter(|_| !text.is_empty()) {
            Some(marked) => [
                TextRun {
                    len: marked.start,
                    ..run.clone()
                },
                TextRun {
                    len: marked.end - marked.start,
                    underline: Some(UnderlineStyle {
                        color: Some(color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: display.len() - marked.end,
                    ..run
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect(),
            None => vec![run],
        };
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display, font_size, &runs, None);

        let width = bounds.size.width;
        let caret_x = if text.is_empty() {
            px(0.0)
        } else {
            line.x_for_index(cursor)
        };
        let caret_width = px(1.5);
        let mut scroll_x = field.scroll_x;
        if caret_x - scroll_x > width - caret_width {
            scroll_x = caret_x - width + caret_width;
        }
        if caret_x < scroll_x {
            scroll_x = caret_x;
        }
        let max_scroll = (line.width + caret_width - width).max(px(0.0));
        scroll_x = scroll_x.clamp(px(0.0), max_scroll);

        let left = bounds.left() - scroll_x;
        let selection = (!selection.is_empty()).then(|| {
            fill(
                Bounds::from_corners(
                    point(left + line.x_for_index(selection.start), bounds.top()),
                    point(left + line.x_for_index(selection.end), bounds.bottom()),
                ),
                field.colors.selection,
            )
        });
        let caret = Some(fill(
            Bounds::new(
                point(left + caret_x, bounds.top()),
                size(caret_width, bounds.size.height),
            ),
            field.colors.caret,
        ));
        Prepainted {
            line,
            selection,
            caret,
            scroll_x,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        prepainted: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.field.read(cx).focus_handle.clone();
        let focused = focus_handle.is_focused(window);
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.field.clone()),
            cx,
        );
        let scroll_x = prepainted.scroll_x;
        let line = &prepainted.line;
        let selection = prepainted.selection.take().filter(|_| focused);
        let caret = prepainted
            .caret
            .take()
            .filter(|_| focused && selection.is_none());
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            if let Some(selection) = selection {
                window.paint_quad(selection);
            }
            line.paint(
                point(bounds.left() - scroll_x, bounds.top()),
                window.line_height(),
                gpui::TextAlign::Left,
                None,
                window,
                cx,
            )
            .ok();
            if let Some(caret) = caret {
                window.paint_quad(caret);
            }
        });
        let line = prepainted.line.clone();
        self.field.update(cx, |field, _| {
            field.layout = Some(line);
            field.bounds = Some(bounds);
            field.scroll_x = scroll_x;
        });
    }
}
