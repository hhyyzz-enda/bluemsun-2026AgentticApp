#!/usr/bin/env python3
"""Test the real in-app WebKit view with local HTML, redirects, and history."""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import socket
import subprocess
import threading
import time
import uuid
from PIL import Image
from native_probe import NativeApp


class BrowserApp(NativeApp):
    window = None

    def request(self, route, **params):
        if self.window is not None and route in {"/snap", "/g", "/click", "/m", "/k", "/close"}:
            params.setdefault("w", self.window)
        return super().request(route, **params)


class Pages(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_GET(self):
        if self.path == "/redirect":
            self.send_response(302)
            self.send_header("Location", "/first")
            self.end_headers()
            return
        if self.path == "/image.png":
            body = Path("resources/icon_64.png").read_bytes()
            content_type = "image/png"
        else:
            name = "Second page" if self.path == "/second" else "First page"
            body = f'''<!doctype html><html><head><title>{name}</title>
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <style>body{{font:18px system-ui;margin:40px;color:#202428;min-height:1600px}}
                article{{max-width:620px;margin:auto}}h1{{font-size:36px}}
                section{{padding:24px;background:#eff7f4;border-radius:16px}}
                img{{width:96px;height:96px}}a{{color:#16704a}}</style></head>
                <body><article><h1>{name}</h1><section><img src="/image.png">
                <p>A real webpage rendered inside Rinx.</p><p id="script">Waiting</p>
                <p>Document visit: {uuid.uuid4().hex}</p>
                <a href="/second">Read the second page</a></section>
                <script>document.getElementById('script').textContent='JavaScript is running';</script>
                </article></body></html>'''.encode()
            content_type = "text/html; charset=utf-8"
        self.send_response(200)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, default=Path("target/fast/examples/chat_web_browser"))
    parser.add_argument("--desktop", action="store_true")
    args = parser.parse_args()
    root = (Path("target/chat-web-browser-regressions") / uuid.uuid4().hex).resolve()
    root.mkdir(parents=True)
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Pages)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        port = probe.getsockname()[1]
    app = BrowserApp(root, port, auto_login=False)
    app.output.mkdir(parents=True)
    app.log = (app.output / "native.log").open("w")
    app.process = subprocess.Popen([str(args.binary.resolve())],
        env=dict(os.environ, MAKEPAD_REMOTE=str(port), MAKEPAD_HIDE_WINDOWS="1", MAKEPAD_NO_FOCUS="1",
                 RINX_DATA_DIR=str(root / "profile"), RAYON_NUM_THREADS="1",
                 RINX_BROWSER_FIXTURE_URL=f"http://127.0.0.1:{server.server_port}",
                 RINX_BROWSER_FIXTURE_MODE="desktop" if args.desktop else "modal",
                 RINX_BROWSER_FIXTURE_OUTPUT=str(root),
                 RINX_BROWSER_FIXTURE_SESSION=str(root / "reader-session.json")),
        stdin=subprocess.DEVNULL, stdout=app.log, stderr=subprocess.STDOUT)
    report = {"passed": False, "mode": "desktop" if args.desktop else "modal", "checks": [], "binary_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest()}
    try:
        for _ in range(100):
            if app.process.poll() is not None:
                raise RuntimeError("Native browser fixture exited")
            try:
                if app.request("/s")["pid"] == app.process.pid:
                    break
            except OSError:
                pass
            time.sleep(.25)
        app.wait_text("Browsers: 0")
        main_window = app.request("/s")["w"][0]["i"]
        app.window = main_window

        def windows(count):
            for _ in range(40):
                current = app.request("/s")["w"]
                if len(current) == count:
                    return current
                time.sleep(.1)
            raise AssertionError(f"Expected {count} windows, got {current}")

        def open_reader():
            app.window = main_window
            app.click_id("open_link")
            if args.desktop:
                app.window = next(w["i"] for w in windows(2) if w["i"] != main_window)
            app.wait_text("Opened link:")

        def close_reader(native=False):
            if native:
                app.request("/close")
            else:
                app.click_id("web_close")
            app.window = main_window
            windows(1)
            app.wait_text("Browsers: 0")
            assert not any(w["i"] == "web_close" for w in app.snap())

        def inspect(title, scroll=False):
            output = root / "webkit.json"
            for _ in range(20):
                output.unlink(missing_ok=True)
                app.click_id("fixture_scroll" if scroll else "fixture_inspect")
                for _ in range(100):
                    if output.exists():
                        try:
                            data = json.loads(output.read_text())
                            break
                        except json.JSONDecodeError:
                            pass
                    time.sleep(.1)
                else:
                    raise RuntimeError("WebKit did not return its document inspection")
                if data.get("title") == title and data.get("readyState") == "complete":
                    return data
                time.sleep(.25)
            raise AssertionError(data)

        def tabs():
            return sorted((w for w in app.snap() if w["ty"] == "Tab"), key=lambda w: w["r"][0])

        def select_tab(tab_id):
            tab = next(t for t in tabs() if t["i"] == tab_id)
            x, y, width, height = tab["r"]
            app.click(x + max(45, width * .65), y + height / 2)

        def close_tab(tab_id):
            tab = next(t for t in tabs() if t["i"] == tab_id)
            x, y, _, height = tab["r"]
            # The native Tab places its close glyph before the title.
            app.click(x + 19, y + height / 2)

        def browser_count(count):
            if args.desktop:
                reader = app.window
                app.window = main_window
                app.wait_text(f"Browsers: {count}")
                app.window = reader

        open_reader()
        first = inspect("First page")
        assert "JavaScript is running" in first["text"]
        assert first["images"] and all(i["complete"] and i["width"] > 0 for i in first["images"])
        assert first["viewportWidth"] > 500 and first["viewportHeight"] > 300
        window_height = next(w["sz"][1] for w in app.request("/s")["w"] if w["i"] == app.window)
        assert first["viewportHeight"] > window_height - 150, first
        report["checks"].append("chat_link_opens_embedded_webkit_with_redirect_css_javascript_and_image")
        # Font atlas uploads may need another frame after the first native draw.
        time.sleep(.5)
        app.capture("browser-toolbar")
        controls = {w["i"]: w for w in app.request("/snap", all=1)["s"]
                    if w["i"] in {"web_close", "web_back", "web_forward", "web_reopen", "web_external"}}
        assert len(controls) == 5, controls
        assert all(w["r"][2:] == [36, 36] and not w.get("t") for w in controls.values()), controls
        report["checks"].append("five_svg_toolbar_controls_have_consistent_hit_areas_without_text_buttons")
        x, y, width, height = controls["web_back"]["r"]
        app.request("/m", x=x + width / 2, y=y + height / 2, k="move", wait=1)
        app.wait_text("Back")
        app.capture("browser-tooltip")
        if args.desktop:
            reader_window = app.window
            app.window = main_window
            app.click_id("chat_action")
            app.wait_text("Chat clicked")
            app.wait_text("Browsers: 1")
            app.window = reader_window
            report["checks"].append("separate_reader_window_keeps_chat_interactive")
        first_tab = tabs()[0]["i"]
        app.click_id("fixture_next")
        saved = inspect("Second page", scroll=True)
        assert saved["scrollY"] >= 300, saved
        if args.desktop:
            open_reader()
            assert app.window == reader_window
        else:
            app.click_id("fixture_new_tab")
        second = inspect("First page")
        assert len(tabs()) == 2, tabs()
        second_tab = next(t["i"] for t in tabs() if t["i"] != first_tab)
        browser_count(2)
        report["checks"].append("another_chat_link_opens_a_new_tab_in_the_same_reader")
        app.capture("browser-tabs")
        select_tab(first_tab)
        restored = inspect("Second page")
        assert restored["text"] == saved["text"], (saved, restored)
        assert abs(restored["scrollY"] - saved["scrollY"]) < 2, restored
        report["checks"].append("switching_tabs_preserves_document_and_scroll_without_reloading")
        app.click_id("web_back")
        inspect("First page")
        app.click_id("web_forward")
        inspect("Second page")
        report["checks"].append("back_and_forward_navigate_real_webkit_history")
        select_tab(second_tab)
        assert inspect("First page")["text"] == second["text"]
        report["checks"].append("tab_histories_are_independent")
        close_tab(first_tab)
        assert len(tabs()) == 1
        assert inspect("First page")["text"] == second["text"]
        browser_count(1)
        report["checks"].append("closing_background_tab_keeps_active_page_and_releases_browser")
        app.click_id("fixture_new_tab")
        inspect("First page")
        third_tab = next(t["i"] for t in tabs() if t["i"] != second_tab)
        close_tab(third_tab)
        assert inspect("First page")["text"] == second["text"]
        browser_count(1)
        report["checks"].append("closing_active_tab_returns_to_surviving_page")
        app.click_id("fixture_next")
        inspect("Second page")
        app.click_id("web_reopen")
        inspect("First page")
        report["checks"].append("reopen_loads_original_chat_link")
        web_tab = tabs()[0]["i"]
        website = inspect("First page")
        app.click_id("fixture_markdown")
        app.wait_text("Shared notes 1.md")
        app.wait_text("Shared Markdown")
        app.capture("markdown-document")
        reader = next(w for w in app.snap() if w['i'] == 'article_reader')
        window_width = next(w['sz'][0] for w in app.request('/s')['w'] if w['i'] == app.window)
        left, _, width, _ = reader['r']
        assert width <= 760.5 and abs(left - (window_width-left-width)) <= 2, reader
        app.wait_text("Tables", pixels=True)
        app.wait_text("answer", pixels=True)
        assert len(tabs()) == 2
        first_doc = next(t["i"] for t in tabs() if t["i"] != web_tab)
        assert not any(w["i"] == "web_external" for w in app.snap())
        browser_count(1)
        app.capture("markdown-document")
        report["checks"].append("markdown_renders_headings_table_and_code_in_native_document_tab")
        link = next(row for row in app.ocr() if "Open webpage" in row["text"])
        width, height = app.request("/s", w=app.window)["w"][0]["sz"]
        x, y, w, h = link["box"]
        app.click((x + w / 2) * width, (y + h / 2) * height)
        inspect("First page")
        assert len(tabs()) == 3
        linked_tab = next(t["i"] for t in tabs() if t["i"] not in {web_tab, first_doc})
        close_tab(linked_tab)
        app.wait_text("Shared Markdown")
        report["checks"].append("markdown_http_link_opens_one_web_tab_and_leaves_document_open")
        app.click_id("fixture_next")
        def document_paragraphs():
            return sorted(w.get("t", "") for w in app.snap() if "Document 1 paragraph" in w.get("t", ""))
        saved_paragraphs = document_paragraphs()
        assert saved_paragraphs, app.snap()
        app.click_id("fixture_markdown")
        app.wait_text("Shared notes 2.md")
        app.wait_text("Document 2 paragraph 1")
        second_doc = next(t["i"] for t in tabs() if t["i"] not in {web_tab, first_doc})
        select_tab(first_doc)
        assert document_paragraphs() == saved_paragraphs
        select_tab(web_tab)
        assert inspect("First page")["text"] == website["text"]
        select_tab(second_doc)
        app.wait_text("Document 2 paragraph 1")
        report["checks"].append("multiple_documents_preserve_scroll_and_coexist_with_live_webpages")
        def appearance():
            app.request('/event', data='theme:inspect', wait=1)
            data=json.loads((root / 'appearance.json').read_text())
            assert data['toolbar_ink'] == data['ink'], data
            return data
        previous_appearance=appearance()
        stable_tabs = [(t['i'],t.get('t')) for t in tabs()]
        for mode in ('dark','light'):
            app.request('/event', data=f'theme:{mode}', wait=1)
            time.sleep(.6)
            new_appearance=appearance()
            assert new_appearance['revision'] != previous_appearance['revision']
            assert new_appearance['toolbar_ink'] != previous_appearance['toolbar_ink']
            assert new_appearance['toolbar_uid'] == previous_appearance['toolbar_uid']
            previous_appearance=new_appearance
            assert [(t['i'],t.get('t')) for t in tabs()] == stable_tabs
            app.wait_text('Document 2 paragraph 1')
            select_tab(first_doc)
            assert document_paragraphs() == saved_paragraphs
            select_tab(web_tab)
            assert inspect('First page')['text'] == website['text'], 'Appearance reloaded external website'
            select_tab(second_doc)
            app.wait_text('Tables', pixels=True)
            app.capture(f'themed-{mode}-documents')
            code=next(w for w in app.snap() if w['ty']=='MarkdownCode')
            x,y,w,h=code['r']
            png=Image.open(root / f'themed-{mode}-documents.png').convert('RGB')
            logical=next(w['sz'] for w in app.request('/s')['w'] if w['i']==app.window)
            pixel=png.getpixel((int((x+w-5)*png.width/logical[0]),int((y+5)*png.height/logical[1])))
            expected=new_appearance['code_bg']
            expected=((expected>>16)&255,(expected>>8)&255,expected&255)
            assert all(abs(a-b)<5 for a,b in zip(pixel,expected)), (mode,pixel,expected,code)
            ink = new_appearance['code_fg']
            ink = ((ink >> 16) & 255, (ink >> 8) & 255, ink & 255)
            sx, sy = png.width / logical[0], png.height / logical[1]
            text_pixels = png.crop((int((x+8)*sx), int((y+6)*sy),
                                    int((x+min(w-8,300))*sx), int((y+h-6)*sy)))
            assert sum(all(abs(a-b)<5 for a,b in zip(p,ink)) for p in text_pixels.getdata()) > 10, (mode,ink,code)

        report['checks'].append('live_theme_preserves_reader_window_tabs_inactive_document_scroll_and_external_website')
        close_tab(first_doc)
        app.wait_text("Document 2 paragraph 1")
        close_tab(second_doc)
        assert inspect("First page")["text"] == website["text"]
        browser_count(1)
        report["checks"].append("closing_document_tabs_preserves_remaining_documents_and_website")
        app.click_id("fixture_markdown")
        app.wait_text("Shared notes 3.md")
        close_reader()
        report["checks"].append("close_destroys_native_browser_and_returns_to_chat")
        open_reader()
        inspect("First page")
        close_tab(tabs()[0]["i"])
        app.window = main_window
        windows(1)
        app.wait_text("Browsers: 0")
        report["checks"].append("closing_last_tab_closes_reader_and_can_reopen_after_cleanup")
        if args.desktop:
            open_reader()
            app.click_id("fixture_new_tab")
            inspect("First page")
            close_reader(native=True)
            report["checks"].append("native_window_close_destroys_all_tabs_and_keeps_chat")
            open_reader()
            inspect("First page")
            app.click_id("fixture_new_tab")
            inspect("First page")
            assert len(tabs()) == 2
            app.window = main_window
            app.request("/close")
            app.process.wait(timeout=10)
            report["checks"].append("closing_main_window_closes_reader_and_exits")
            saved = json.loads((root / "reader-session.json").read_text())
            assert len(saved["tabs"]) == 2 and saved["active"] == 1, saved

            def restart_fixture():
                # A new process must actually rebuild the native window/tabs from disk.
                app._activity_checked = False
                app._user_seq = None
                app.window = None
                app.process = subprocess.Popen([str(args.binary.resolve())],
                    env=dict(os.environ, MAKEPAD_REMOTE=str(port), MAKEPAD_HIDE_WINDOWS="1", MAKEPAD_NO_FOCUS="1",
                             RINX_DATA_DIR=str(root / "profile"), RAYON_NUM_THREADS="1",
                             RINX_BROWSER_FIXTURE_URL=f"http://127.0.0.1:{server.server_port}",
                             RINX_BROWSER_FIXTURE_MODE="desktop",
                             RINX_BROWSER_FIXTURE_OUTPUT=str(root),
                             RINX_BROWSER_FIXTURE_SESSION=str(root / "reader-session.json")),
                    stdin=subprocess.DEVNULL, stdout=app.log, stderr=subprocess.STDOUT)
                for _ in range(100):
                    if app.process.poll() is not None:
                        raise RuntimeError("Restarted browser fixture exited")
                    try:
                        if app.request("/s")["pid"] == app.process.pid:
                            return
                    except OSError:
                        pass
                    time.sleep(.25)
                raise RuntimeError("Restarted browser fixture did not start")

            restart_fixture()
            current = windows(2)
            main_window = min(w["i"] for w in current)
            app.window = main_window
            app.wait_text("Browsers: 2")
            app.window = next(w["i"] for w in current if w["i"] != main_window)
            app.wait_text("Opened link:")
            # A restored window appears during Startup, before macOS supplies its
            # caption inset. Let the first native geometry/draw cycle settle
            # before using the inspector's button coordinates for a click.
            time.sleep(.6)
            assert len(tabs()) == 2
            inspect("First page")
            assert any(w.get("t", "").endswith("/first") for w in app.snap() if "Opened link:" in w.get("t", ""))
            report["checks"].append("restart_restores_reader_window_tab_order_and_active_link_from_disk")
            close_reader()
            app.request("/close")
            app.process.wait(timeout=10)
            assert not json.loads((root / "reader-session.json").read_text())["tabs"]
            restart_fixture()
            app.window = windows(1)[0]["i"]
            app.wait_text("Browsers: 0")
            assert not any(w["i"] == "web_close" for w in app.snap())
            report["checks"].append("explicitly_closed_reader_stays_closed_after_restart")
        report["passed"] = True
    finally:
        app.process.terminate()
        app.process.wait(timeout=10)
        app.log.close()
        server.shutdown()
        (root / "report.json").write_text(json.dumps(report, indent=2))
        (root / "trace.json").write_text(json.dumps(app.trace, indent=2))
        print(json.dumps({"report": str(root / "report.json"), **report}))


if __name__ == "__main__":
    main()
