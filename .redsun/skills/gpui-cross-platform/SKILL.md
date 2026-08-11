---
name: gpui-cross-platform
description: >
  Keeps GPUI apps working on macOS, Windows, and Linux when you can only test
  on one of them. GPUI's cross-platform failures are silent — a featureless
  backend, an absent font family, or an unset app_id all compile clean and
  produce a window that panics, renders nothing, or has no identity. Use when
  touching anything in the window shell (WindowOptions, titlebar, decorations,
  resize, menus, keybindings, fonts, icons, packaging), when bumping the gpui
  revision, when adding a feature that must look right on a platform you cannot
  run, or when a user reports "works on my machine, broken on theirs" for a
  GPUI app. Also use whenever a GPUI window opens with no titlebar, no icon,
  wrong or missing text, or refuses to resize or move.
---

# Cross-platform GPUI

## The operative fact

**GPUI's cross-platform failures do not produce compile errors.** They produce a
binary that builds, links, and runs — and then does the wrong thing, or nothing,
on a platform you were not looking at. Every bug in the "known traps" table below
shipped in a release that built green on CI.

This is the difference from Electron: there is no shared runtime papering over
the platforms. GPUI's macOS, Windows, and Linux backends are separate crates with
separate feature sets and different levels of completeness, and the seams are
opt-in rather than opt-out. Budget for that instead of being surprised by it.

So the rule is: **you cannot conclude a platform works because the code compiles
for it.** Prove it from the artifact or from the backend's source. Sections
"Verify without the machine" and "Read the backend, not the docs" are how.

## Read the backend, not the docs

The `gpui` facade exposes methods that are no-ops or unimplemented on some
backends. The signature tells you nothing. Before relying on any window/platform
API, open the backend for the platform you cannot test:

```
~/.cargo/git/checkouts/zed-*/<rev>/crates/
  gpui/                  # facade + shared types (ContentMask, Modifiers, Tiling)
  gpui_platform/         # feature flags that select a backend  <- read this first
  gpui_linux/src/linux/{wayland,x11,headless}/window.rs
  gpui_macos/src/{platform,window}.rs
  gpui_windows/src/window.rs
```

Check the trait default in `gpui/src/platform.rs` too — an unimplemented method
often silently inherits a do-nothing default rather than failing to compile.

Zed itself is the reference implementation for the hard parts. If you are
unsure how a window-shell concern should be handled, find it in `crates/zed`,
`crates/workspace`, `crates/platform_title_bar`, or `crates/theme` and follow
that shape. Copy the *reasoning*, not the code: verify each API still exists at
the revision this project pins.

## Verify without the machine

Four techniques, in order of how much they buy you.

### 1. Prefer `cfg!` over `#[cfg]`

`#[cfg(target_os = "macos")]` code is **not compiled** on your dev machine, so a
type error in it survives until someone builds on a Mac. A runtime `if cfg!(...)`
branch compiles everywhere and is optimized out, giving you type-checking of the
other platforms' code for free.

```rust
// Bad: never type-checked on the machine you develop on.
#[cfg(target_os = "macos")]
{ cx.set_menus(mac_menus()); }

// Good: compiles (and is checked) everywhere, runs only on macOS.
if cfg!(target_os = "macos") {
    cx.set_menus(mac_menus());
}
```

Same for helper functions: leave them ungated and gate only the call. Reserve
`#[cfg]` for code that genuinely cannot compile off-platform (platform crates,
`objc`, win32 bindings).

### 2. Interrogate the built artifact

A backend that was never compiled in leaves no trace in the binary. This is the
fastest way to prove a Linux/macOS build is real, and it works on a release
artifact downloaded from CI:

```bash
# Which shared libraries does it actually want?
grep -a -o 'lib[A-Za-z0-9_.+-]*\.so\(\.[0-9]*\)*' ./echo-desktop | sort -u

# Marker strings that prove (or disprove) a backend is present.
for s in xkbcommon vulkan wayland headless NoopTextSystem; do
  printf '%-16s %s\n' "$s" "$(grep -a -c "$s" ./echo-desktop)"
done
```

A GUI binary whose library list is identical to your CLI binary's has no GUI in
it. Zero hits for `xkbcommon` on a Linux build means no windowing backend.
`NoopTextSystem` present in a macOS build means it will render no text.

### 3. Gate it in CI

Any invariant you just proved by hand belongs in the release workflow, because
the failure mode is a clean build:

```yaml
- name: Assert the Linux GUI backend is linked
  if: runner.os == 'Linux'
  run: ldd target/release/<binary> | grep -q libxkbcommon
```

### 4. Ask for a targeted report, not "it's broken"

When you must rely on someone else's machine, ask for the specific observation
that discriminates between causes, and always capture the environment (see
"Reporting a windowing bug").

## Known traps

Verified against the gpui revision this project pins. **Re-check every one when
bumping the revision** — backends move between crates and features get renamed.

| Trap | Symptom | What to do |
|---|---|---|
| `gpui_platform` has `default = []` | Linux panics out of `current_platform()` before opening a window; macOS substitutes `NoopTextSystem` and draws no text. Both build clean. Windows is unaffected, so dev-on-Windows never sees it. | Name the features explicitly: `features = ["wayland", "x11", "font-kit"]`. Enabling them also requires system dev packages in CI (`libxkbcommon-x11-dev`, `libwayland-dev`, `libx11-xcb-dev`, `libfontconfig-dev`). |
| Font families match by exact name | No generic `monospace` alias and no substitution on a miss — the text silently falls back to the proportional UI font, which shears any fixed-pitch layout (ASCII art, aligned columns). `"Consolas"` is Windows-only; `"Menlo"` is macOS-only. | Resolve at runtime against `cx.text_system().all_font_names()` with a per-platform candidate list; cache the result. |
| `ContentMask` is a plain rectangle | Rounded corners do not cascade. `overflow_hidden` on a rounded container does not clip children, so any descendant painting an opaque background into a corner squares it off. | Round *every* surface that reaches a corner (root background, titlebar, whatever is at the bottom), not just a wrapper. |
| `window_control_area` is inert on Linux | `on_hit_test_window_control` is unimplemented in the Wayland, X11, and headless backends, so caption buttons tagged for Min/Max/Close do nothing. Works on Windows via `WM_NCHITTEST`. | Call `minimize_window`/`zoom_window` directly on Linux; route close through your Quit action. Use `.occlude()` or the surrounding drag hitbox eats the click. |
| `set_app_identity` does not set the Linux app identity | On Linux it only names notifications. With `WindowOptions.app_id` unset the window announces no identity, so the desktop shows a blank icon and "Unknown" — while the launcher entry still looks correct, because installed hicolor icons only reach the app grid. | Set `app_id`, and keep it equal to the `.desktop` file's basename *and* its `StartupWMClass` (the X11 half of the same match). |
| `window_decorations` is a request | GNOME/Mutter does not implement xdg-decoration and forces client-side decorations — no titlebar, no border, no resize handles. X11 with no compositor does the reverse: a `Client` request is downgraded to `Server` and the WM draws its own bar. | Branch on `window.window_decorations()` at render time, never on `cfg!(linux)` alone. Render your custom titlebar only when you actually own the frame, or you get two on some setups and none on others. |
| Maximize implies tiling | Both Linux backends report `Tiling::tiled()` when maximized or fullscreen. | Drop shadow margins, corner rounding, and resize handles on tiled edges, or a maximized window floats with a transparent gap. |
| `Modifiers::control` is not the platform chord | macOS users press cmd. A guard testing `.control` silently ignores cmd-V — which breaks pasting into any hand-rolled text field, including first-run credential entry. | Use `Modifiers::secondary()` (cmd on macOS, ctrl elsewhere). Accept both if you also mirror a terminal keymap. |
| `remove_window` bypasses `on_window_should_close` | State you persist on close (window bounds, drafts) is silently lost through that path. | Route every exit — caption button, keybinding, menu item — through one action that persists, then quits. |
| macOS has no default menu bar | GPUI builds `NSApp.mainMenu` only from `set_menus`; without it the bar beside the Apple menu is empty. No Quit item, so cmd-Q does nothing, and no Window menu, so cmd-M does nothing. | Supply a menu. The menu bar is a separate surface from your in-window titlebar — having a titlebar does not give you one. |

## Window contract

Decide these explicitly for every top-level window rather than inheriting a
default: `is_resizable`, `is_movable`, `is_minimizable`, `window_min_size`,
`window_bounds`, `titlebar`, `window_decorations`, `app_id`.

The root UI must survive being made small and large — no fixed dimensions, no
overflow that fights the window size. Confirm the minimum size is actually
reachable.

## Decoration and titlebar policy

Treat the titlebar as three implementations behind one look:

- **macOS** — native traffic lights; inset the bar past them (~71px). Do not draw
  your own close/minimize/maximize. `appears_transparent` hides the system bar;
  `app_owns_titlebar_drag` means you must implement drag yourself, via a
  mouse-down latch that calls `start_window_move` on the first move.
- **Windows** — `window_control_area` tags give you drag, double-click, and snap
  layouts from `WM_NCHITTEST` for free. Keep custom controls clear of the system
  resize regions.
- **Linux** — assume nothing. Query `window_decorations()`. Under client-side
  decorations you owe the user: drag, double-click maximize/restore, right-click
  system menu, working caption buttons, a border, and resize on all eight edges
  (`start_window_resize`). Requesting `Client` decorations makes the surface
  transparent, which is what lets rounded corners show through.

## Test matrix

Per platform, the smoke tests that catch the traps above:

- **macOS** — menu bar present with working cmd-Q/H/M; traffic-light clearance;
  drag and double-click on the bar; resize; fullscreen; **text renders at all**;
  cmd-V into every text field.
- **Windows** — caption buttons; snap/maximize; drag region; resize; DPI change.
- **Linux (Wayland)** — window opens at all; titlebar present exactly once; drag;
  caption buttons; resize from every edge and corner; maximize is flush and
  square; correct icon and app name in the dock/alt-tab; fixed-pitch text aligns.
- **Linux (X11)** — the same, plus the no-compositor case, where the WM draws the
  frame and your custom bar must not also appear.

## Change discipline

- Keep app logic platform-neutral; confine OS-specific behavior to the window
  shell so there is one place to audit.
- Every platform-specific fix gets either a CI assertion (preferred — see
  "Gate it in CI") or a line in the manual checklist above.
- State the tested OS, session type (Wayland/X11), and gpui revision in the PR.

## Reporting a windowing bug

Ask for, or record: distro, desktop environment, compositor, session type
(Wayland or X11), GPU driver, gpui revision, and whether the app was launched
from a terminal or a desktop entry. That last one matters more than it looks —
a desktop launcher discards stderr, so a startup panic is indistinguishable from
the icon doing nothing. Running the binary directly from a terminal is usually
the single most informative step, and installing a panic hook that writes to a
log file under the app's config directory makes the difference permanent.
