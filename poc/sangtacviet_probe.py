"""
M0 POC: probe sangtacviet.vip to determine what it takes to get chapter text.

Outputs (all under poc/out/):
  shell.html     - HTML of the page before any interaction
  rendered.html  - HTML after the load-chapter click + text materializes
  xhr.jsonl      - one line per network response, with body when text-ish
  selectors.md   - notes the script wrote: click selector, body selector, etc.
  screenshot.png - final viewport, for over-the-shoulder verification

Run:
  pip install playwright
  playwright install chromium
  python3 poc/sangtacviet_probe.py [URL]

If URL is omitted, uses the seed target from the plan.
"""

from __future__ import annotations

import json
import re
import sys
import time
from pathlib import Path

from playwright.sync_api import Page, Request, Response, sync_playwright

SEED_URL = "https://sangtacviet.vip/truyen/yushubo/1/134050/1/"
OUT = Path(__file__).parent / "out"
OUT.mkdir(parents=True, exist_ok=True)

UA = (
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) "
    "AppleWebKit/537.36 (KHTML, like Gecko) "
    "Chrome/124.0.0.0 Safari/537.36"
)

LOAD_TRIGGER_PATTERNS = [
    re.compile(r"Nhấp vào để tải chương", re.I),
    re.compile(r"tải chương", re.I),
    re.compile(r"load chapter", re.I),
    re.compile(r"点击加载", re.I),
]

TEXTY_CT = ("text/", "application/json", "application/xml", "application/javascript")


def log(msg: str) -> None:
    print(f"[probe] {msg}", flush=True)


def attach_network_recorder(page: Page, jsonl_path: Path) -> None:
    fh = jsonl_path.open("w", encoding="utf-8")

    def on_response(resp: Response) -> None:
        req: Request = resp.request
        entry = {
            "url": resp.url,
            "status": resp.status,
            "method": req.method,
            "resource_type": req.resource_type,
            "request_headers": dict(req.headers),
            "response_headers": dict(resp.headers),
        }
        ct = resp.headers.get("content-type", "")
        if any(t in ct for t in TEXTY_CT) and resp.status < 400:
            try:
                body = resp.text()
                if len(body) > 200_000:
                    body = body[:200_000] + "\n/* TRUNCATED */"
                entry["body"] = body
            except Exception as e:
                entry["body_error"] = str(e)
        fh.write(json.dumps(entry, ensure_ascii=False) + "\n")
        fh.flush()

    page.on("response", on_response)


def find_load_trigger(page: Page):
    """Return a Locator pointing at the load-chapter trigger, or None."""
    for pat in LOAD_TRIGGER_PATTERNS:
        loc = page.get_by_text(pat).first
        try:
            if loc.count() > 0:
                return loc
        except Exception:
            pass

    for sel in [
        "#loadchapter",
        ".loadchapter",
        "[onclick*='load']",
        "[onclick*='chap']",
        "button:has-text('chương')",
        "a:has-text('chương')",
    ]:
        loc = page.locator(sel).first
        try:
            if loc.count() > 0:
                return loc
        except Exception:
            pass
    return None


def guess_body_selector(page: Page) -> tuple[str | None, int]:
    """Find the element on the page whose text content is longest (likely chapter body)."""
    result = page.evaluate(
        """
        () => {
            const all = Array.from(document.querySelectorAll('body *'));
            let best = null, bestLen = 0;
            for (const el of all) {
                if (el.children.length > 5) continue; // prefer leaf-ish containers
                const t = (el.innerText || '').trim();
                if (t.length > bestLen) {
                    bestLen = t.length;
                    best = el;
                }
            }
            if (!best) return { selector: null, length: 0 };
            // Build a stable-ish selector
            const parts = [];
            let node = best;
            while (node && node.nodeType === 1 && parts.length < 5) {
                let sel = node.tagName.toLowerCase();
                if (node.id) { sel += '#' + node.id; parts.unshift(sel); break; }
                if (node.className && typeof node.className === 'string') {
                    const cls = node.className.trim().split(/\\s+/).slice(0, 2).join('.');
                    if (cls) sel += '.' + cls;
                }
                parts.unshift(sel);
                node = node.parentElement;
            }
            return { selector: parts.join(' > '), length: bestLen };
        }
        """
    )
    return result.get("selector"), result.get("length", 0)


def run(url: str) -> int:
    log(f"target: {url}")
    with sync_playwright() as p:
        browser = p.chromium.launch(
            headless=True,
            args=["--disable-blink-features=AutomationControlled"],
        )
        ctx = browser.new_context(user_agent=UA, locale="en-US")
        ctx.add_cookies([
            {"name": "foreignlang", "value": "vi", "domain": "sangtacviet.vip", "path": "/"},
            {"name": "transmode", "value": "name", "domain": "sangtacviet.vip", "path": "/"},
        ])
        ctx.add_init_script(
            """
            Object.defineProperty(navigator, 'webdriver', { get: () => undefined });

            // Record every XHR open() so we can see what URLs the site prepares.
            window.__xhrOpens = [];
            const origOpen = XMLHttpRequest.prototype.open;
            XMLHttpRequest.prototype.open = function(method, url, ...rest) {
                window.__xhrOpens.push({ method, url, ts: Date.now() });
                return origOpen.call(this, method, url, ...rest);
            };
            """
        )
        page = ctx.new_page()

        attach_network_recorder(page, OUT / "xhr.jsonl")

        log("navigating...")
        try:
            page.goto(url, wait_until="commit", timeout=60_000)
            page.wait_for_load_state("domcontentloaded", timeout=15_000)
        except Exception as e:
            log(f"navigation warning: {e} (continuing, DOM may still be usable)")

        # Let all the `stv.*.js` scripts finish registering click handlers.
        try:
            page.wait_for_load_state("networkidle", timeout=20_000)
        except Exception as e:
            log(f"networkidle timeout (continuing): {e}")

        (OUT / "shell.html").write_text(page.content(), encoding="utf-8")
        log(f"saved shell.html ({len(page.content())} bytes)")

        # Dismiss any residual modal (cookies should prevent it, but belt-and-suspenders).
        try:
            page.evaluate(
                "document.querySelectorAll('.modal-backdrop, .modal.show, .modal').forEach(e => e.remove())"
            )
        except Exception:
            pass

        page.wait_for_timeout(3000)
        click_selector = "#maincontent"

        diag_before = page.evaluate(
            """
            () => ({
                webdriver: navigator.webdriver,
                xhrSendToString: XMLHttpRequest.prototype.send.toString(),
                xhrSendIsNative: /\\[native code\\]/.test(XMLHttpRequest.prototype.send.toString()),
                chapterfetcherReadyState: typeof chapterfetcher !== 'undefined' ? chapterfetcher.readyState : null,
                maincontentText: (document.getElementById('maincontent')||{}).innerText?.slice(0,120) || null,
            })
            """
        )
        log(f"before trigger: {json.dumps(diag_before, ensure_ascii=False)}")

        opens_before = page.evaluate("window.__xhrOpens || []")
        log(f"XHR opens before trigger: {json.dumps(opens_before, ensure_ascii=False)[:400]}")

        # Try a real mouse click first — Playwright's .click() synthesises a trusted
        # user gesture in most cases.
        try:
            page.click("#maincontent", force=True, timeout=5_000)
        except Exception as e:
            log(f"mouse click error (continuing): {e}")

        page.wait_for_timeout(2000)
        opens_after = page.evaluate("window.__xhrOpens || []")
        log(f"XHR opens after click: {json.dumps(opens_after, ensure_ascii=False)[:600]}")

        cookies = ctx.cookies()
        (OUT / "cookies.json").write_text(json.dumps(cookies, indent=2, ensure_ascii=False))
        log(f"wrote cookies.json ({len(cookies)} cookies)")

        log("waiting for chapter text to materialize...")
        deadline = time.time() + 20
        body_selector, body_len = None, 0
        while time.time() < deadline:
            body_selector, body_len = guess_body_selector(page)
            if body_len >= 500:
                break
            time.sleep(0.5)

        log(f"longest text node: {body_len} chars @ {body_selector}")

        (OUT / "rendered.html").write_text(page.content(), encoding="utf-8")
        log(f"saved rendered.html ({len(page.content())} bytes)")
        page.screenshot(path=str(OUT / "screenshot.png"), full_page=False)

        notes = [
            f"# sangtacviet.vip POC notes",
            "",
            f"- URL: {url}",
            f"- Load-chapter click selector: `{click_selector or 'NOT FOUND'}`",
            f"- Longest text block selector: `{body_selector or 'NOT FOUND'}`",
            f"- Longest text block length: {body_len} chars",
            f"- Shell HTML size: {(OUT / 'shell.html').stat().st_size} bytes",
            f"- Rendered HTML size: {(OUT / 'rendered.html').stat().st_size} bytes",
            "",
            "## Verdict",
            "",
            (
                "- Static fetch sufficient"
                if body_len >= 500
                and "Nhấp vào" not in (OUT / "shell.html").read_text(encoding="utf-8")
                else "- Headless required (shell HTML did not contain chapter text; click was needed)"
            ),
            "",
            "## Next actions",
            "",
            "- grep xhr.jsonl for the endpoint that returned the chapter text; if one exists, SangtacvietPlugin may be able to skip chromiumoxide after an initial session warm-up.",
            "- Verify URL arithmetic by rerunning this script against chapter 2 and 3 URLs.",
        ]
        (OUT / "selectors.md").write_text("\n".join(notes), encoding="utf-8")
        log(f"wrote selectors.md")

        browser.close()
    log("done")
    return 0


if __name__ == "__main__":
    target = sys.argv[1] if len(sys.argv) > 1 else SEED_URL
    sys.exit(run(target))
