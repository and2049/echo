//! The window's large panes as their own views, so gpui can reuse a pane's last frame while
//! another one repaints.
//!
//! A cached view is redrawn only when it is notified itself: gpui does not redraw it because
//! an entity it read has changed. Every pane reads [`EchoApp`], so a notify on the app redraws
//! all of them, and [`Regions::notify`] redraws one pane for changes that are known to stay
//! inside it.

use gpui::{
    AnyElement, App, AppContext, Context, Empty, Entity, IntoElement, ParentElement, Render,
    StyleRefinement, Styled, WeakEntity, Window, div, px,
};

use crate::{EchoApp, views};

/// Height of the playback bar. Cached views are laid out from a style rather than measured, so
/// the bar and the slot holding it both size themselves from this.
pub(crate) const PLAYBACK_BAR_HEIGHT: f32 = 108.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Region {
    Sidebar,
    Main,
    RightPanel,
    PlaybackBar,
}

pub(crate) struct RegionView {
    app: WeakEntity<EchoApp>,
    region: Region,
}

impl Render for RegionView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let region = self.region;
        let content = self
            .app
            .update(cx, |app, cx| -> AnyElement {
                match region {
                    Region::Sidebar => views::sidebar(app, cx).into_any_element(),
                    Region::Main => views::main_area(app, window, cx).into_any_element(),
                    Region::RightPanel => match app.right_panel {
                        Some(panel) => views::right_panel(app, panel, window, cx),
                        None => Empty.into_any_element(),
                    },
                    Region::PlaybackBar => app.render_playback_bar(cx).into_any_element(),
                }
            })
            .unwrap_or_else(|_| Empty.into_any_element());
        // A cached view's content is laid out as a root, which shrinks to fit; the column makes
        // it fill the slot the parent gave it instead.
        div().size_full().flex().flex_col().child(content)
    }
}

pub(crate) struct Regions {
    sidebar: Entity<RegionView>,
    main: Entity<RegionView>,
    right_panel: Entity<RegionView>,
    playback_bar: Entity<RegionView>,
}

impl Regions {
    pub(crate) fn new(cx: &mut Context<EchoApp>) -> Self {
        let app = cx.weak_entity();
        let mut view = |region| {
            let app = app.clone();
            cx.new(|_| RegionView { app, region })
        };
        Self {
            sidebar: view(Region::Sidebar),
            main: view(Region::Main),
            right_panel: view(Region::RightPanel),
            playback_bar: view(Region::PlaybackBar),
        }
    }

    fn view(&self, region: Region) -> &Entity<RegionView> {
        match region {
            Region::Sidebar => &self.sidebar,
            Region::Main => &self.main,
            Region::RightPanel => &self.right_panel,
            Region::PlaybackBar => &self.playback_bar,
        }
    }

    /// Redraws one pane and leaves the others' cached frames alone.
    pub(crate) fn notify(&self, region: Region, cx: &mut Context<EchoApp>) {
        App::notify(cx, self.view(region).entity_id());
    }

    pub(crate) fn notify_all(&self, cx: &mut Context<EchoApp>) {
        for region in [
            Region::Sidebar,
            Region::Main,
            Region::RightPanel,
            Region::PlaybackBar,
        ] {
            self.notify(region, cx);
        }
    }

    pub(crate) fn sidebar(&self, width: f32) -> impl IntoElement + use<> {
        self.sidebar
            .clone()
            .cached(StyleRefinement::default().flex_none().w(px(width)).h_full())
    }

    pub(crate) fn main(&self) -> impl IntoElement + use<> {
        self.main.clone().cached(
            StyleRefinement::default()
                .flex_grow(1.0)
                .h_full()
                .overflow_hidden(),
        )
    }

    pub(crate) fn right_panel(&self, width: f32) -> impl IntoElement + use<> {
        self.right_panel
            .clone()
            .cached(StyleRefinement::default().flex_none().w(px(width)).h_full())
    }

    pub(crate) fn playback_bar(&self) -> impl IntoElement + use<> {
        self.playback_bar.clone().cached(
            StyleRefinement::default()
                .flex_none()
                .w_full()
                .h(px(PLAYBACK_BAR_HEIGHT)),
        )
    }
}
