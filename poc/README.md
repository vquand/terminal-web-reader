# POC — fetch-strategy probe

Throwaway scripts used to figure out what each target site actually needs
before we commit plugin code in Rust. Artifacts land in `poc/out/` and
become test fixtures for the `tests/fixtures/` directory later.

## Setup (one-time)

```bash
python3 -m venv .venv
source .venv/bin/activate
pip install playwright
playwright install chromium
```

## Run the sangtacviet probe

```bash
python3 poc/sangtacviet_probe.py
# or another chapter URL:
python3 poc/sangtacviet_probe.py https://sangtacviet.vip/truyen/yushubo/1/134050/2/
```

## What to share back

After a run, paste the contents of `poc/out/selectors.md` and the first
~20 lines of `poc/out/xhr.jsonl` (grep for any URL that looks like a
content endpoint — `json`, `getchap`, `loadchap`, etc.). That's enough to
lock in `SangtacvietPlugin` and move on to M1.

## Gotchas

- If navigation fails with `ERR_CONNECTION_CLOSED`, the site is blocking
  the chromium egress (corp proxy, geo filter). Try with a VPN or from a
  different network — this is the same class of problem the Rust binary
  will face at runtime, so it's worth resolving here.
- `selectors.md` picks "longest text node on the page" as a rough proxy
  for the chapter body. Always eyeball `rendered.html` to confirm.
