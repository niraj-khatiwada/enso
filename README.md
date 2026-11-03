<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/logo-light.png">
    <img src="assets/logo-dark.png" alt="enso logo" width="128">
  </picture>
</p>

<h1 align="center">enso</h1>

<p align="center">A native Markdown studio for writing. Edit right in the typeset page, keep plain <code>.md</code> files on disk. Built in Rust with GPUI Kit, with no web view.</p>

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/screenshots/visual-dark.png">
    <img src="assets/screenshots/visual-light.png" alt="A note open in enso's visual editor" width="880">
  </picture>
</p>

## Write in the page

Open any Markdown file and you see it typeset: headings, emphasis, lists, task lists, quotes, tables, code blocks, links and images. Click anywhere and type. There's no preview pane: the page you read is the page you edit, and the Markdown behind it changes only where you typed.

- **Selection that feels native:** drag across blocks with edge autoscroll, double-click for a word, triple-click for a paragraph, Shift-click or Shift-arrows to extend.
- **Copy as Markdown:** the clipboard gets the exact source behind your selection, so pasting into another Markdown file round-trips.
- **Formatting shortcuts:** bold, italic, strikethrough, code, links, headings, lists, task lists, quotes and code blocks. Each applies to the selection or the word at the caret, and pressing it again removes it.
- **Lists that continue:** Enter starts the next bullet, task or number; Enter on an empty item ends the list. Tab and Shift-Tab nest and un-nest.
- **Links and footnotes:** `[text][id]` resolves through its definition anywhere in the file, and editing the definition updates every link that uses it. ⌘-click (Ctrl-click) or ⌥Enter follows a link, and jumps from a footnote to its note and back.
- **One undo history:** undo and redo work the same in every view, and restore where you were.

<p align="center"><img src="assets/screenshots/select-light.png" alt="A paragraph selected in the visual editor" width="880"></p>

## Source when you want it

⌘E switches between the page and syntax-highlighted Markdown. ⌘\\ puts them side by side, and pressing it again returns to the view you came from. The caret stays on the same character across views and stays at the same height on screen, so you never lose your place. In side by side, typing on either side updates the other immediately.

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/screenshots/side-dark.png">
    <img src="assets/screenshots/side-light.png" alt="Source and visual views side by side" width="880">
  </picture>
</p>

<p align="center"><img src="assets/screenshots/source-light.png" alt="The Markdown source view" width="880"></p>

## Writing modes

**Typewriter mode** keeps the line you are writing at one height on the screen, so your eyes stay put while the page moves. A dashed guide marks the line; drag it up or down and it settles back to the centre when you release near it. ⌘⇧T turns it on and off.

<p align="center"><img src="assets/screenshots/typewriter-light.png" alt="Typewriter mode with the writing line on its guide" width="880"></p>

**Focus** dims everything except the sentence or paragraph you are writing, and fades softly as you move between them. Sentences end where you expect, even after abbreviations like "Dr." or "e.g." and in Devanagari after the danda (।). ⌘⇧D cycles through off, sentence and paragraph.

<p align="center"><img src="assets/screenshots/focus-light.png" alt="Sentence focus dimming the rest of the page" width="880"></p>

While you type, the sidebar, tabs and status bar fade away and return as soon as you move the pointer or press Escape. The text column never moves.

## Images

Images sit in the page at their natural size, up to the width of the column. Click one to select it: drag the handle to resize it, align it left, centre or float it right, or open the image dialog to change its file, description and width. ⌘⇧I inserts a new image from a file or a URL, and pasting a screenshot saves it next to your notes.

enso writes a plain Markdown image whenever it can, and an HTML `<img>` tag only when a size or alignment needs one. The dialog tells you which it will write.

<p align="center"><img src="assets/screenshots/image-light.png" alt="A selected image with its toolbar and resize handle" width="880"></p>

<p align="center"><img src="assets/screenshots/image-dialog-light.png" alt="The image dialog" width="880"></p>

## Tables

Click into a table and edit cells directly, including bold, links and code. Tab and Shift-Tab move between cells, and Tab on the last cell starts a new row. A small toolbar adds and removes rows and columns and tidies the column padding in the Markdown, all without moving the table.

<p align="center"><img src="assets/screenshots/table-light.png" alt="Editing a table with its toolbar" width="880"></p>

## Front matter

YAML and TOML front matter opens in its own panel above the page, with syntax highlighting. Edit it there and the header of your file changes with it; fold the panel away when you don't need it.

<p align="center"><img src="assets/screenshots/frontmatter-light.png" alt="The front matter panel above the page" width="880"></p>

## Make it yours

Choose the caret's colour, thickness, blink rate and blink pattern, the size of the text in the page, and how the page behaves while you write. Light and dark themes follow the system, or pick one yourself.

<p align="center">
  <img src="assets/screenshots/settings-writing-light.png" alt="Writing settings" width="430">
  <img src="assets/screenshots/settings-caret-light.png" alt="Caret settings" width="430">
</p>

## Your files, untouched

- Documents are plain `.md` files. Opening a file and closing it without edits never writes it, byte for byte, including line endings, whitespace, list markers and emphasis style.
- A visual edit changes only the bytes of the blocks you touched. CRLF files stay CRLF.
- Saves happen automatically after 500 ms of quiet, are written in the background one at a time, and are flushed when you quit. ⌘S saves at once.
- If a file changes on disk while you have unsaved edits, enso keeps your text and tells you instead of overwriting either version.
- No account, no analytics, no network access except loading images your documents reference.

## Fast on long documents

Typing is built to fit inside a 120 Hz frame on documents of hundreds of kilobytes: edits reparse only the block they touch, and long lists and tables re-render only the rows that changed. Opening a long file shows the first screen at once and parses the rest in the background. Files over 256 KiB open in a section reader that lays out only what is on screen, and their source opens on demand.

## A workspace for notes

<p align="center"><img src="assets/screenshots/workspace-light.png" alt="A workspace folder open in enso" width="880"></p>

- Open a folder as a workspace: browse it lazily, create, rename and delete files and folders (deletes go to the Trash or Recycle Bin).
- Tabs keep their edits while you switch between files and folders. Several windows can be open at once.
- Paste a screenshot or an image file and enso saves it into the workspace and links it.
- Relative images and links resolve against the document's folder. Web links open in your browser.
- On macOS, Quick Look previews Markdown files from Finder with the Space bar.

## Keyboard shortcuts

⌘ on macOS, Ctrl on Windows and Linux.

| App | |
|---|---|
| ⌘O / ⌘⇧O | Open file / open folder as a workspace |
| ⌘N / Ctrl⇧N | New file / new folder in the selected folder |
| Enter (F2 on Windows/Linux) | Rename the selected file or folder |
| ⌘⌫ (Delete on Windows/Linux) | Move the selected file or folder to the Trash |
| ⌘S | Save now |
| ⌘W | Close the current file |
| ⌘E | Switch between the page and the source |
| ⌘\\ | Side by side; press again to return |
| ⌘, | Settings |
| ⌘Q | Quit |

| Writing | |
|---|---|
| ⌘B / ⌘I | Bold / italic |
| ⌘⇧X | Strikethrough |
| ⌘⇧C | Inline code |
| ⌘K | Link |
| ⌘1 – ⌘6, ⌘0 | Heading level 1–6, back to body text |
| ⌘⇧8 / ⌘⇧7 / ⌘⇧L | Bullet, numbered or task list |
| ⌘Enter | Tick or untick a task |
| ⌘⇧. | Quote |
| ⌘⌥C | Code block |
| Tab / ⇧Tab | Nest / un-nest a list item |
| ⌘A, ⌘C, ⌘X, ⌘V | Select all, copy, cut and paste Markdown |
| ⌘Z / ⌘⇧Z | Undo / redo |
| ⌥Enter | Follow the link or footnote at the caret |
| ⌘⇧I | Insert or edit an image |
| Tab / ⇧Tab in a table | Next / previous cell |
| ⌘⌥↑ / ⌘⌥↓ | Insert a table row above / below |
| ⌘⌥← / ⌘⌥→ | Insert a table column left / right |
| ⌘⌥⌫ / ⌘⌥⇧⌫ | Delete the table row / column |
| ⌘⌥T | Tidy the table's column padding |
| ⌘⇧T | Typewriter mode |
| ⌘⇧D | Focus: off, sentence, paragraph |
| ⌘= / ⌘- | Larger / smaller text |
| Escape | Bring back the sidebar and tabs |

The source editor also has ⌘F find, ⌘⇧F replace, ⌘] / ⌘[ to indent and outdent lines, and ⌥⌘↑ / ⌥⌘↓ to add cursors.

## Build

Install Rust with rustup; `rust-toolchain.toml` pins the version.

```sh
cargo run
cargo run -- /absolute/path/to/note.md
cargo run -- /absolute/path/to/folder
```

On macOS, `make` lists the build targets. `make build.macos` produces `target/enso.app` with its Quick Look extension, ad-hoc signed, and a disk image. `make install` copies the app to `/Applications`. `make bench` runs the editor benchmarks.

On the first launch without arguments, enso opens an editable `Get Started.md` from its application data folder.

`.github/workflows/release.yml` builds release packages from the Actions tab: a macOS disk image for Apple silicon, Windows zips for x64 and ARM64, and Linux tarballs for x64 and ARM64. On Linux, unpack the tarball and run `./install.sh` to add enso to the app launcher.

## Platforms

One codebase for macOS, Windows and Linux on GPUI Kit's native backends. Linux needs GPUI's native build dependencies: a C/C++ toolchain, pkg-config, fontconfig, Wayland and X11 development libraries, and a working Vulkan driver.

GPUI Base is built from a pinned fork (`gpui-base-patched` in this repository) that exposes the editor host hooks the visual editor uses. `vendor/gpui-pre-macos` is the upstream 0.3.10 crate with three `Option::map_or_default` calls replaced by stable equivalents.

## Tests

```sh
cargo fmt --check
cargo check --locked
cargo test --locked
cargo test --locked --features ui-tests
./scripts/check-tokens.sh
```

Sizes, spacing, opacities and motion come from `src/tokens.rs`; the check above fails on new literal values anywhere else.
