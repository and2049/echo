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
# One slot per semantic role — selection, hover, muted fill, border, accent hover, accent
# selection — over the window background, then the same roles over the elevated surface.
DERIVED = [
    ("row_selected", "highlight_bg", 0.20, "background", "Selected list-row pill (highlight_bg 20% over background)."),
    ("row_hover", "text_muted", 0.10, "background", "Any hovered row over the window: sidebar, lists, quick links, range pills (text_muted 10% over background)."),
    ("wash", "text_muted", 0.15, "background", "Muted fills: placeholder cover boxes, small button hovers, disabled buttons (text_muted 15% over background)."),
    ("border", "text_muted", 0.30, "background", "Window-level borders, pill outlines and the seek/volume bar track (text_muted 30% over background)."),
    ("accent_hover", "primary", 0.12, "background", "Accent-tinted hover: tabs, links, active transport toggles (primary 12% over background)."),
    ("accent_selected", "primary", 0.20, "background", "Accent-tinted selection: drop targets, resize handle, suggestion chip (primary 20% over background)."),
    ("like_dim", "secondary", 0.25, "background", "The faint heart on a not-liked track; hovering shows secondary itself (secondary 25% over background)."),
    ("error_wash", "error", 0.15, "background", "The audio-device failure banner fill (error 15% over background)."),
    ("menu_hover", "text_muted", 0.10, "surface", "Hovered row, button or titlebar caption in menus and modals (text_muted 10% over surface)."),
    ("menu_selected", "highlight_bg", 0.20, "surface", "Selected menu/picker row (highlight_bg 20% over surface)."),
    ("menu_accent", "primary", 0.12, "surface", "Accent-tinted hover/selection in pickers, suggestions and modal controls (primary 12% over surface)."),
    ("menu_border", "text_muted", 0.35, "surface", "Borders in and around menus, modals, popovers and drag chips (text_muted 35% over surface)."),
    ("danger_border", "error", 0.50, "surface", "Confirm-prompt destructive button border (error 50% over surface)."),
    ("danger_wash", "error", 0.12, "surface", "That button's hover fill (error 12% over surface)."),
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
        value = blend(resolved[base], resolved[under], alpha)
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
