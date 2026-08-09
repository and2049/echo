#!/usr/bin/env python3
"""Regenerate the `[desktop]` derived-color table in every bundled theme.

The desktop app paints a set of washes, borders and hover tints that used to be runtime
transparency effects (`color.opacity(a)` over a background). They are now concrete colors:
each theme file spells every one out under `[desktop]`, and this script computes them from
the base slots with the exact math the app uses as a fallback (`echo-desktop/src/theme.rs`,
`DesktopPalette::resolve` — plain sRGB-channel interpolation, verified against on-screen
pixels).

Run it after changing any base color in a theme:

    python themes/generate_desktop.py

It rewrites every `themes/*.toml` in place, preserving the base values and refreshing the
whole `[desktop]` table plus the per-key comments.
"""

import re
import sys
import tomllib
from pathlib import Path

# The desktop's fixed palette for named ANSI colors (echo-desktop/src/theme.rs `named`).
NAMED = {
    "black": (0x00, 0x00, 0x00),
    "red": (0xCD, 0x31, 0x31),
    "green": (0x0D, 0xBC, 0x79),
    "yellow": (0xE5, 0xE5, 0x10),
    "blue": (0x24, 0x72, 0xC8),
    "magenta": (0xBC, 0x3F, 0xBC),
    "cyan": (0x11, 0xA8, 0xCD),
    "gray": (0xA0, 0xA0, 0xA0),
    "grey": (0xA0, 0xA0, 0xA0),
    "darkgray": (0x66, 0x66, 0x66),
    "darkgrey": (0x66, 0x66, 0x66),
    "lightred": (0xF1, 0x4C, 0x4C),
    "lightgreen": (0x23, 0xD1, 0x8B),
    "lightyellow": (0xF5, 0xF5, 0x43),
    "lightblue": (0x3B, 0x8E, 0xEA),
    "lightmagenta": (0xD6, 0x70, 0xD6),
    "lightcyan": (0x29, 0xB8, 0xDB),
    "white": (0xE5, 0xE5, 0xE5),
}

# Window defaults standing in for `Reset` (echo-desktop/src/theme.rs).
WINDOW_BG = (0x12, 0x12, 0x12)
WINDOW_FG = (0xE5, 0xE5, 0xE5)
PANEL_BG = (0x1F, 0x1F, 0x22)

# Fixed reds (CLOSE_RED / DANGER_RED in echo-desktop/src/theme.rs).
CLOSE_RED = (0xE8, 0x11, 0x23)


def hsl_to_rgb(h, s, l):
    c = (1 - abs(2 * l - 1)) * s
    x = c * (1 - abs((h / 60) % 2 - 1))
    m = l - c / 2
    r, g, b = {0: (c, x, 0), 1: (x, c, 0), 2: (0, c, x), 3: (0, x, c), 4: (x, 0, c), 5: (c, 0, x)}[
        int(h // 60) % 6
    ]
    return tuple(round((v + m) * 255) for v in (r, g, b))


DANGER_RED = hsl_to_rgb(0.0, 0.7, 0.6)


def xterm256(i):
    if i < 16:
        order = [
            "black", "red", "green", "yellow", "blue", "magenta", "cyan", "gray",
            "darkgray", "lightred", "lightgreen", "lightyellow", "lightblue",
            "lightmagenta", "lightcyan", "white",
        ]
        return NAMED[order[i]]
    if i < 232:
        i -= 16
        step = lambda v: 0 if v == 0 else 55 + v * 40
        return (step(i // 36), step((i % 36) // 6), step(i % 6))
    v = 8 + (i - 232) * 10
    return (v, v, v)


def parse_color(value, reset):
    normalized = re.sub(r"[ _-]", "", value).lower()
    if normalized in ("reset", "default"):
        return reset
    if normalized in NAMED:
        return NAMED[normalized]
    if normalized.startswith("#") and len(normalized) == 7:
        return tuple(int(normalized[i : i + 2], 16) for i in (1, 3, 5))
    if normalized.isdigit():
        return xterm256(int(normalized))
    return None


def blend(color, under, alpha):
    return tuple(round(c * alpha + u * (1 - alpha)) for c, u in zip(color, under))


def hexstr(rgb):
    return "#%02x%02x%02x" % rgb


# Base slots: (key, parse fallback, Reset stand-in, comment). Order is the file order.
BASE = [
    ("primary", NAMED["cyan"], WINDOW_FG, "Accent: active tabs, the seek bar, links, the playing-track marker."),
    ("secondary", NAMED["yellow"], WINDOW_FG, "Second accent: liked-track hearts, the condensed lyric line's neighbours."),
    ("background", None, WINDOW_BG, "The window background."),
    ("surface", None, PANEL_BG, "Elevated panels: menus, modals, popovers, the titlebar."),
    ("text", NAMED["white"], WINDOW_FG, "Main text."),
    ("text_muted", NAMED["darkgray"], WINDOW_FG, "Secondary text: artist/album columns, hints, timestamps, inactive icons."),
    ("highlight_bg", NAMED["white"], WINDOW_FG, "Selection source color; selected rows derive their fill from this."),
    ("highlight_fg", NAMED["black"], WINDOW_FG, "Text on strong highlights (the TUI's selected rows)."),
    ("error", NAMED["red"], WINDOW_FG, "Errors: failure messages and the audio-device banner."),
]

# Derived slots: (key, base, alpha, underlay, comment). Order mirrors DesktopPalette.
DERIVED = [
    ("row_selected", "highlight_bg", 0.20, "background", "Selected list-row pill (highlight_bg 20% over background)."),
    ("row_hover", "text_muted", 0.10, "background", "Hovered sidebar row / quick link / range pill (text_muted 10% over background)."),
    ("row_hover_faint", "text_muted", 0.08, "background", "Hovered main-area list row (text_muted 8% over background)."),
    ("wash_muted", "text_muted", 0.15, "background", "Placeholder cover boxes, small round-button hover, disabled buttons (text_muted 15% over background)."),
    ("bar_track", "text_muted", 0.25, "background", "Seek and volume bar track (text_muted 25% over background)."),
    ("border_soft", "text_muted", 0.30, "background", "Window-level borders: sidebar edge, bar tops, input outlines (text_muted 30% over background)."),
    ("outline", "text_muted", 0.40, "background", "Inactive pill outlines in the main area (text_muted 40% over background)."),
    ("accent_wash", "primary", 0.10, "background", "Hovered tab or link (primary 10% over background)."),
    ("accent_wash_icon", "primary", 0.15, "background", "Hovered active-state transport toggle (primary 15% over background)."),
    ("accent_wash_strong", "primary", 0.20, "background", "Drag-over drop targets and the sidebar resize handle (primary 20% over background)."),
    ("suggestion_selected", "primary", 0.25, "background", "Selected command-bar suggestion chip (primary 25% over background)."),
    ("wash_fg", "text", 0.15, "background", "Hovered play/pause and prev/next buttons (text 15% over background)."),
    ("like_dim", "secondary", 0.25, "background", "The faint heart on a not-liked track (secondary 25% over background)."),
    ("like_hover", "secondary", 0.70, "background", "That heart while hovered (secondary 70% over background)."),
    ("error_wash", "error", 0.15, "background", "The audio-device failure banner fill (error 15% over background)."),
    ("menu_hover", "text_muted", 0.10, "surface", "Hovered menu/modal row (text_muted 10% over surface)."),
    ("menu_row_selected", "highlight_bg", 0.20, "surface", "Selected context/track-menu row (highlight_bg 20% over surface)."),
    ("menu_selected", "text_muted", 0.18, "surface", "Selected theme-picker row (text_muted 18% over surface)."),
    ("menu_border_soft", "text_muted", 0.30, "surface", "Input and segmented-control outlines inside modals (text_muted 30% over surface)."),
    ("border_strong", "text_muted", 0.40, "surface", "Popover, modal and drag-chip borders (text_muted 40% over surface)."),
    ("picker_selected", "primary", 0.12, "surface", "Selected/hovered row in pickers, menus and suggestions (primary 12% over surface)."),
    ("menu_accent_wash", "primary", 0.10, "surface", "Hovered control inside modals (primary 10% over surface)."),
    ("prompt_confirm_border", None, 0.50, "surface", "Confirm-prompt destructive button border (fixed danger red 50% over surface)."),
    ("prompt_confirm_wash", None, 0.12, "surface", "That button's hover fill (fixed danger red 12% over surface)."),
    ("prompt_cancel_border", "text_muted", 0.50, "surface", "Confirm-prompt cancel button border (text_muted 50% over surface)."),
    ("prompt_cancel_wash", "text_muted", 0.12, "surface", "That button's hover fill (text_muted 12% over surface)."),
    ("titlebar_hover", "text", 0.08, "surface", "Titlebar minimize/maximize caption hover (text 8% over surface)."),
    ("titlebar_active", "text", 0.12, "surface", "Those captions while pressed (text 12% over surface)."),
    ("close_active", None, 0.80, "surface", "The close caption while pressed (Windows red 80% over surface; its hover is the system red itself)."),
]


def regenerate(path):
    raw = tomllib.loads(path.read_text(encoding="utf-8"))

    resolved = {}
    for key, fallback, reset, _comment in BASE:
        value = raw.get(key)
        color = parse_color(value, reset) if isinstance(value, str) else None
        if color is None:
            color = fallback if fallback is not None else reset
        resolved[key] = color

    lines = []
    for key, _fallback, _reset, comment in BASE:
        value = raw.get(key)
        if value is None:
            continue
        lines.append(f"# {comment}")
        lines.append(f'{key} = "{value}"')
    lines.append("")
    lines.append("# Derived colors the desktop app paints — concrete values, no runtime")
    lines.append("# transparency. Each comment states the formula they were generated with;")
    lines.append("# edit freely, or delete a key (or this whole table) to fall back to the")
    lines.append("# formula. Regenerate after changing base colors: python themes/generate_desktop.py")
    lines.append("[desktop]")
    for key, base, alpha, under, comment in DERIVED:
        if key == "close_active":
            src = CLOSE_RED
        elif key.startswith("prompt_confirm"):
            src = DANGER_RED
        else:
            src = resolved[base]
        value = blend(src, resolved[under], alpha)
        lines.append(f"# {comment}")
        lines.append(f'{key} = "{hexstr(value)}"')
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"{path.name}: regenerated")


def main():
    here = Path(__file__).parent
    for path in sorted(here.glob("*.toml")):
        regenerate(path)


if __name__ == "__main__":
    main()
