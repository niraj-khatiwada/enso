# gpui-excalidraw

A project that starts from scratch and uses Rust to re implement [Excalidraw](https://github.com/excalidraw/excalidraw), built on top of [GPUI Kit](https://gpui-kit.com), but removed certain features.

## Overview

This is a **single Rust library crate** that reimplements Excalidraw — the hand-drawn style virtual whiteboard — with a runnable desktop application example. The data model, serialization format, and interaction model faithfully mirror the upstream Excalidraw project.

### Key characteristics

- **Pure Rust library** (lib name `gpui_excalidraw`) consumable by downstream GPUI Kit applications.
- **1:1 data model** — element types, fill styles, stroke styles, arrowheads, bindings, and the `.excalidraw` JSON schema match Excalidraw's `packages/element/src/types.ts` exactly.
- **i18n** — 25 locales (English, Bahasa Indonesia, Deutsch, Español, Euskara, Français, Italiano, Nederlands, Polski, Português, Português Brasileiro, Română, Slovenčina, Slovenščina, Svenska, Türkçe, Русский, Українська, עברית, العربية, فارسی, 日本語, 简体中文, 繁體中文, 한국어) with runtime language switching.
- **Theming** — light/dark theme switching mapped onto GPUI Kit's semantic design tokens.
- **Hand-drawn rendering** — a roughjs-style renderer (hachure / cross-hatch / zigzag / solid fills, deterministic seeded jitter) producing the signature Excalidraw "sketchy" look.
- **Full editing interaction** — selection (marquee + click), drawing (rectangle / diamond / ellipse / arrow / line / freedraw / text / image / frame / sticky note), move / resize / rotate handles, multi-point line & arrow authoring (click-click-Enter or double-click, draggable vertices), endpoint binding (arrows/lines auto-attach to shape outlines and re-glue when the shape moves or rotates), alt-drag duplicate, grid snapping, Shift angle / square constraints, clipboard (copy / cut / paste / select-all), space-drag pan, zoom-to-fit / zoom-to-selection, text inline editing, eraser (drag), laser pointer, pan, zoom-to-cursor, undo/redo, delete, duplicate.
- **Menus** — top-left file menu (new / save `.excalidraw` / load / export PNG / export SVG), a right-click canvas context menu (duplicate, copy/cut/paste, select all, bring to front, send to back), a **command palette** (Ctrl+/, fuzzy-filterable and localized), a **find-on-canvas bar** (Ctrl+F, match count + prev/next stepper), and a **keyboard-shortcut dialog** (?) listing every binding.
- **Element operations** — lock/unlock, group/ungroup, align (left/center/right/top/middle/bottom), distribute (horizontal/vertical), layer reordering (bring forward/send backward/front/back).
- **Properties panel** — right-side floating panel to edit stroke/background color, fill style, stroke width, stroke style, opacity, roughness, roundness, arrowheads (start/end), and text properties (font family, font size, text alignment).
- **Library panel** — save selected elements to a reusable library and insert them back onto the canvas.
- **Layers panel** — element list with selection, reordering, and deletion.
- **Keyboard shortcuts** — tool switching (V/H/R/D/O/A/L/P/T/F/E/N/K/0), Ctrl+Z/Y undo/redo, Ctrl+C/X/V clipboard, Ctrl+A select-all, Ctrl+D duplicate, Ctrl+G group, Ctrl+S save, Ctrl+/ command palette, Ctrl+F find on canvas, ? shortcut dialog, Ctrl+Shift+E export PNG, Enter to commit a polyline, Delete/Backspace, Shift+1/2 zoom-to-fit/selection.
- **Export & persistence** — SVG export (hachure/cross-hatch/zigzag fills clipped to the shape outline, arrowheads, rounded corners, solid fills, multi-line text via `<tspan>`, and embedded images as `<image href="data:…">`), real **PNG export** (the SVG is rasterized with `resvg`/`usvg`, so the bytes are an actual bitmap at 1x/2x scale, not an SVG renamed), and `.excalidraw` JSON serialization/deserialization compatible with the official format (including the top-level `files` map), plus round-trip load.
- **Tests** — unit tests, integration tests (including headless GPUI UI tests), and runnable examples.

## Layout

The whole package lives at the repository root:

```
Cargo.toml             Single package: lib `gpui_excalidraw` + example `excalidraw_example`
build.rs               Embeds assets/icons/*.svg into `ui::icons`
assets/                icons/ (140 upstream glyphs) + fonts/ (Excalifont, Virgil)
src/
  lib.rs               Crate root: `pub mod core` / `render` / `ui` + top-level re-exports
  core/                Data model, geometry, element types, scene, collision, transform
  render/              Hand-drawn (roughjs-style) renderer + SVG/PNG exporters
  ui/                  GPUI Kit editor: toolbar, canvas, interaction, i18n, theme, export
tests/                 Integration tests (core_*, render, ui_*)
examples/              excalidraw_example, verify_features, demo_scene_svg, export_demo
```

The layer directories are declared through `#[path]` in `src/core/lib.rs`,
`src/render/lib.rs`, and `src/ui/lib.rs`, which keeps the physical grouping
(`core/`, `render/`, `ui/`) while the public paths stay short — `gpui_excalidraw::core::…`,
`::render::…`, `::ui::…`.

## Building & running

```bash
# Run the full editor application (requires a desktop environment)
cargo run --example excalidraw_example

# Run all tests (unit + integration + headless UI tests)
cargo test

# Lint
cargo clippy --all-targets
```

### Registry note

Add a `.cargo/config.toml` pointing `crates-io` at the official sparse index if your setup needs it; a global `~/.cargo/config.toml` (e.g. `rsproxy`) is picked up automatically.

## Using as a library

Add to your `Cargo.toml`:

```toml
[dependencies]
gpui-excalidraw = { path = "path/to/gpui-excalidraw" }
gpui-kit = "0.7"
```

### Embed the editor

```rust
use gpui_kit::*;

fn main() {
    gpui_kit::application().run(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
            cx.new(|_| {
                let mut editor = excalidraw::Editor::new();
                editor.add_demo_scene();
                editor
            })
        })
        .expect("failed to open window");
    });
}
```

### Programmatic scene construction & export

```rust
use excalidraw::core::{ElementOptions, new_rectangle, Scene, Point};
use excalidraw::{export_svg, scene_to_json};

let mut scene = Scene::new();
let blue = ElementOptions { stroke_color: "#1971c2".into(), ..Default::default() };
scene.add(new_rectangle(0.0, 0.0, 200.0, 100.0, &blue));

let svg = export_svg(&scene, "#ffffff");
let json = scene_to_json(&scene, &excalidraw::core::AppState::default()).unwrap();
```

### i18n

```rust
use excalidraw::{I18n, Language};
let i18n = I18n::new(Language::ZhCn);
assert_eq!(i18n.t("toolbar.rectangle"), "矩形");
```

## Data model fidelity

The element model replicates Excalidraw's `Element` union exactly, including:

- `rectangle`, `diamond`, `ellipse`, `line`, `arrow`, `freedraw`, `text`, `image`, `frame`, `magicframe`, `iframe`, `embeddable`, `stickynote`, `selection`
- `FillStyle` (`hachure`, `cross-hatch`, `solid`, `zigzag`), `StrokeStyle` (`solid`, `dashed`, `dotted`)
- `Arrowhead` variants (including cardinality / crowfoot notation)
- `FixedPointBinding`, `BoundElement`, `ImageCrop`
- `seed` / `version` / `versionNonce` fields for deterministic rendering and collaboration reconciliation

The `.excalidraw` JSON output uses Excalidraw's exact `camelCase` field names and `{ "type": "excalidraw", "version": 2 }` envelope, so saved files open directly in the official Excalidraw app.

### Runnable examples

```bash
# End-to-end feature verification; writes verify.svg + verify.png + verify.excalidraw
cargo run --example verify_features

# Render the built-in demo scene to demo-scene.svg
cargo run --example demo_scene_svg

# Print an SVG export of a small scene and write diagram.excalidraw
cargo run --example export_demo
```

PNG export is available both from the UI (file menu → 导出图片, or Ctrl+Shift+E)
and programmatically:

```rust
let png = excalidraw::render::export::export_png(&scene, "#ffffff", 2.0)?;
```

## License

MIT
