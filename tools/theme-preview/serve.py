#!/usr/bin/env python3
"""Live preview for echo desktop themes.

Serves a static mock of the desktop window whose every color comes from a theme toml
(9 base slots + the 14-key `[desktop]` table), resolved with the exact math the app
uses — imported straight from `themes/generate_desktop.py`, so the sRGB blend, the
named/indexed color handling and the formula fallbacks can never drift. Watches
`themes/*.toml` and pushes changes to the page over Server-Sent Events: save a file
in your editor and the mock repaints in under a second.

    python tools/theme-preview/serve.py [--port 7910] [--no-open]
"""

import argparse
import importlib.util
import json
import re
import sys
import threading
import time
import tomllib
import webbrowser

sys.dont_write_bytecode = True
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
THEMES_DIR = ROOT / "themes"
ICONS_DIR = ROOT / "crates" / "echo-desktop" / "icons"
INDEX = Path(__file__).with_name("index.html")

_spec = importlib.util.spec_from_file_location(
    "generate_desktop", THEMES_DIR / "generate_desktop.py"
)
gen = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(gen)

NAME_RE = re.compile(r"^[A-Za-z0-9_-]+$")
HEX_RE = re.compile(r"^#[0-9a-fA-F]{6}$")
BASE_COMMENTS = {key: comment for key, _f, _r, comment in gen.BASE}
DERIVED_COMMENTS = {key: comment for key, _b, _a, _u, comment in gen.DERIVED}


def theme_names():
    return sorted(p.stem for p in THEMES_DIR.glob("*.toml"))


def resolve_theme(path):
    raw = tomllib.loads(path.read_text(encoding="utf-8"))

    resolved = {}
    base = {}
    for key, fallback, reset, comment in gen.BASE:
        value = raw.get(key)
        color = gen.parse_color(value, reset) if isinstance(value, str) else None
        if color is None:
            color = fallback if fallback is not None else reset
        resolved[key] = color
        base[key] = {"value": gen.hexstr(color), "comment": comment}

    table = raw.get("desktop")
    if not isinstance(table, dict):
        table = {}
    desktop = {}
    for key, base_key, alpha, under, comment in gen.DERIVED:
        # Overrides live either in a [desktop] table or as flat top-level keys; the
        # table wins, mirroring the app (echo-core/src/theme.rs).
        value = table.get(key)
        if not isinstance(value, str):
            value = raw.get(key)
        color = gen.parse_color(value, None) if isinstance(value, str) else None
        explicit = color is not None
        if color is None:
            color = gen.blend(resolved[base_key], resolved[under], alpha)
        desktop[key] = {
            "value": gen.hexstr(color),
            "comment": comment,
            "fallback": not explicit,
        }
    return {"base": base, "desktop": desktop}


def snapshot_mtimes():
    return {p.stem: p.stat().st_mtime_ns for p in THEMES_DIR.glob("*.toml")}


def write_key(path, key, value):
    """Set one color in a theme file, preserving layout and inline comments.

    Replaces the value on the existing `key = "..."` line wherever it lives (flat or
    inside a `[desktop]` table); appends a commented line when the key is absent —
    base keys go above a `[desktop]` header if the file has one.
    """
    text = path.read_text(encoding="utf-8")
    pattern = re.compile(rf'^(\s*{re.escape(key)}\s*=\s*)"[^"]*"', re.MULTILINE)
    if pattern.search(text):
        text = pattern.sub(lambda m: f'{m.group(1)}"{value}"', text)
    else:
        comment = BASE_COMMENTS.get(key) or DERIVED_COMMENTS.get(key)
        line = f'{key} = "{value}" # {comment}\n'
        if key in BASE_COMMENTS and "[desktop]" in text:
            text = text.replace("[desktop]", f"{line}\n[desktop]", 1)
        else:
            text = text.rstrip("\n") + "\n\n" + line
    path.write_text(text, encoding="utf-8")


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *_args):
        pass

    def send_body(self, body, content_type, status=200):
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-cache")
        self.end_headers()
        self.wfile.write(body)

    def send_json(self, data, status=200):
        self.send_body(json.dumps(data).encode("utf-8"), "application/json", status)

    def do_GET(self):
        try:
            self.route()
        except (ConnectionAbortedError, BrokenPipeError, OSError):
            pass

    def do_POST(self):
        try:
            self.route_post()
        except (ConnectionAbortedError, BrokenPipeError, OSError):
            pass

    def route_post(self):
        path = self.path.split("?", 1)[0]
        matched = re.match(r"^/api/theme/([A-Za-z0-9_-]+)/(set|regenerate)$", path)
        if not matched:
            self.send_json({"error": "not found"}, 404)
            return
        name, action = matched.groups()
        file = THEMES_DIR / f"{name}.toml"
        if not file.is_file():
            self.send_json({"error": "unknown theme"}, 404)
            return
        length = int(self.headers.get("Content-Length") or 0)
        try:
            body = json.loads(self.rfile.read(length) or b"{}")
        except json.JSONDecodeError:
            self.send_json({"error": "bad json"}, 400)
            return
        if action == "set":
            key = body.get("key")
            value = body.get("value")
            if key not in BASE_COMMENTS and key not in DERIVED_COMMENTS:
                self.send_json({"error": "unknown key"}, 400)
                return
            if not isinstance(value, str) or not HEX_RE.match(value):
                self.send_json({"error": "value must be #rrggbb"}, 400)
                return
            write_key(file, key, value.lower())
        else:
            gen.regenerate(file)
        self.send_json({"ok": True})

    def route(self):
        path = self.path.split("?", 1)[0]
        if path == "/":
            self.send_body(INDEX.read_bytes(), "text/html; charset=utf-8")
        elif path == "/api/themes":
            self.send_json(theme_names())
        elif path.startswith("/api/theme/"):
            name = path.removeprefix("/api/theme/")
            file = THEMES_DIR / f"{name}.toml"
            if not NAME_RE.match(name) or not file.is_file():
                self.send_json({"error": "unknown theme"}, 404)
                return
            try:
                self.send_json(resolve_theme(file))
            except (tomllib.TOMLDecodeError, OSError) as err:
                # Half-saved files parse mid-write sometimes; the page just retries.
                self.send_json({"error": str(err)}, 422)
        elif path == "/api/events":
            self.stream_events()
        elif path.startswith("/icons/"):
            name = path.removeprefix("/icons/").removesuffix(".svg")
            file = ICONS_DIR / f"{name}.svg"
            if not NAME_RE.match(name) or not file.is_file():
                self.send_json({"error": "unknown icon"}, 404)
                return
            self.send_body(file.read_bytes(), "image/svg+xml")
        else:
            self.send_json({"error": "not found"}, 404)

    def stream_events(self):
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.end_headers()
        mtimes = snapshot_mtimes()
        last_write = time.monotonic()
        while True:
            time.sleep(0.25)
            current = snapshot_mtimes()
            changed = sorted(
                {n for n in current if mtimes.get(n) != current[n]}
                | {n for n in mtimes if n not in current}
            )
            mtimes = current
            if changed:
                payload = f"data: {json.dumps(changed)}\n\n".encode("utf-8")
            elif time.monotonic() - last_write > 15.0:
                payload = b": ping\n\n"
            else:
                continue
            self.wfile.write(payload)
            self.wfile.flush()
            last_write = time.monotonic()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=7910)
    parser.add_argument("--no-open", action="store_true", help="don't open the browser")
    args = parser.parse_args()

    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    server.daemon_threads = True
    url = f"http://127.0.0.1:{args.port}/"
    print(f"echo theme preview — {url}")
    print(f"watching {THEMES_DIR}\\*.toml (ctrl-c to stop)")
    if not args.no_open:
        threading.Timer(0.3, webbrowser.open, [url]).start()
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
