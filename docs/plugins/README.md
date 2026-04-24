# Shipping plugins

Per-plugin usage docs live here. Writing your own is covered in
[`docs/writing-plugins.md`](../writing-plugins.md).

| Plugin | Handles | Requires |
|---|---|---|
| [`generic`](./generic.md) | Static HTML articles and blog-style chapters with `<link rel="next">` or "next" anchors | — |
| [`sangtacviet`](./sangtacviet.md) | sangtacviet.vip web-novel chapters | `js` feature + Chromium |

Dispatch is first-match, in the order plugins are registered in
`build_registry()` (`src/main.rs`). Specific plugins are registered before
`generic` so their `matches(url)` predicate wins.
