/// Byte index of the char boundary one character before `cursor` (or `cursor` at the start).
fn prev_char_boundary(value: &str, cursor: usize) -> usize {
    value[..cursor.min(value.len())]
        .char_indices()
        .next_back()
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// Byte index of the char boundary one character after `cursor` (or `cursor` at the end).
fn next_char_boundary(value: &str, cursor: usize) -> usize {
    let cursor = cursor.min(value.len());
    value[cursor..]
        .chars()
        .next()
        .map(|c| cursor + c.len_utf8())
        .unwrap_or(cursor)
}

/// Insert `text` into `value` at the byte offset `cursor`, advancing the cursor past it. The
/// shared insertion path for both typed characters and pastes in the hand-rolled fields.
pub(crate) fn insert_at_cursor(value: &mut String, cursor: &mut usize, text: &str) {
    *cursor = (*cursor).min(value.len());
    value.insert_str(*cursor, text);
    *cursor += text.len();
}

/// Apply the cursor-aware editing keys shared by the hand-rolled text fields — arrow keys,
/// Home/End, Backspace, Delete, and printable character insertion — to `value`/`cursor`.
///
/// Returns `true` when the event was one of those keys (so the caller can stop). Enter, Escape,
/// Tab, and paste stay with the caller: each field submits, cancels, and filters pasted text
/// differently. Cursor positions are byte offsets kept on char boundaries so multi-byte queries
/// (e.g. CJK search terms) navigate a character at a time rather than splitting a codepoint.
pub(crate) fn apply_text_edit(
    value: &mut String,
    cursor: &mut usize,
    event: &gpui::KeyDownEvent,
) -> bool {
    *cursor = (*cursor).min(value.len());
    match event.keystroke.key.as_str() {
        "left" => *cursor = prev_char_boundary(value, *cursor),
        "right" => *cursor = next_char_boundary(value, *cursor),
        "home" => *cursor = 0,
        "end" => *cursor = value.len(),
        "backspace" => {
            if *cursor > 0 {
                let start = prev_char_boundary(value, *cursor);
                value.replace_range(start..*cursor, "");
                *cursor = start;
            }
        }
        "delete" => {
            if *cursor < value.len() {
                let end = next_char_boundary(value, *cursor);
                value.replace_range(*cursor..end, "");
            }
        }
        _ => {
            // Insert only genuine typed text: a modifier chord (ctrl-v, cmd-a) is not input, and
            // named keys with no glyph report no `key_char`.
            let modifiers = &event.keystroke.modifiers;
            if modifiers.control || modifiers.platform {
                return false;
            }
            match event.keystroke.key_char.as_deref() {
                Some(text) if !text.is_empty() && !text.chars().any(char::is_control) => {
                    insert_at_cursor(value, cursor, text);
                }
                _ => return false,
            }
        }
    }
    true
}

/// Render a field's text with the block caret drawn at `cursor` (a byte offset), so the hand-rolled
/// inputs show where the next edit lands instead of pinning the caret to the end.
pub(crate) fn text_with_cursor(value: &str, cursor: usize) -> String {
    let mut cursor = cursor.min(value.len());
    while cursor > 0 && !value.is_char_boundary(cursor) {
        cursor -= 1;
    }
    format!("{}▏{}", &value[..cursor], &value[cursor..])
}

#[cfg(test)]
mod text_edit_tests {
    use super::{insert_at_cursor, next_char_boundary, prev_char_boundary, text_with_cursor};

    #[test]
    fn boundaries_step_over_multibyte_chars() {
        // "é" is two bytes, "中" is three: navigation must land on char boundaries, not bytes.
        let value = "aé中b"; // bytes: a(1) é(2) 中(3) b(1)
        assert_eq!(next_char_boundary(value, 0), 1); // past 'a'
        assert_eq!(next_char_boundary(value, 1), 3); // past 'é'
        assert_eq!(next_char_boundary(value, 3), 6); // past '中'
        assert_eq!(next_char_boundary(value, 6), 7); // past 'b'
        assert_eq!(next_char_boundary(value, 7), 7); // clamped at end

        assert_eq!(prev_char_boundary(value, 7), 6);
        assert_eq!(prev_char_boundary(value, 6), 3);
        assert_eq!(prev_char_boundary(value, 3), 1);
        assert_eq!(prev_char_boundary(value, 1), 0);
        assert_eq!(prev_char_boundary(value, 0), 0); // clamped at start
    }

    #[test]
    fn insert_advances_cursor_past_inserted_text() {
        let mut value = "cd".to_string();
        let mut cursor = 0;
        insert_at_cursor(&mut value, &mut cursor, "ab");
        assert_eq!(value, "abcd");
        assert_eq!(cursor, 2);

        // Insert a multi-byte char at the caret; the cursor moves by its byte length.
        insert_at_cursor(&mut value, &mut cursor, "é");
        assert_eq!(value, "abécd");
        assert_eq!(cursor, 4);
    }

    #[test]
    fn caret_renders_at_cursor_and_survives_stale_index() {
        assert_eq!(text_with_cursor("abc", 0), "▏abc");
        assert_eq!(text_with_cursor("abc", 1), "a▏bc");
        assert_eq!(text_with_cursor("abc", 3), "abc▏");
        // A byte index past the end is clamped rather than panicking.
        assert_eq!(text_with_cursor("abc", 9), "abc▏");
        // An index landing mid-codepoint snaps back to a boundary.
        assert_eq!(text_with_cursor("é", 1), "▏é");
    }
}
