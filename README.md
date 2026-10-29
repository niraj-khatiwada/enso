# Enso

A native, reading-first Markdown workspace built in Rust with GPUI Kit. No webview or Tauri.

## Run

Install stable Rust (1.95 or later). Keep the supplied GPUI Kit repository beside this project:

```text
Documents/
  enso/
  gpui-kit-main/
```

```sh
cargo run
cargo run -- /absolute/path/to/note.md
cargo run -- /absolute/path/to/folder
```

On macOS, `make` lists the build targets. `make build.macos` produces the release: an optimized `target/Enso.app` with its Quick Look extension (Space-bar previews of Markdown in Finder), ad-hoc signed, and `target/Enso-<version>-<arch>.dmg`. `make app` stops at the app, `make app.debug` makes a quick unoptimized one, and `make install` copies it to `/Applications` and registers it and its Quick Look preview. The bundle uses `assets/Enso.icns`; `assets/enso.png` also supplies the Dock icon during `cargo run`.

The initial build downloads and compiles GPUI's native dependencies. Subsequent builds are incremental. Development builds optimize GPUI Base, Ropey, and the text index to avoid slow million-line editor initialization under `cargo run`; the first rebuild after a profile change takes longer.

## First version

On the first launch without a file/folder argument, Enso creates and opens an editable `Get Started.md` in its local application data folder (`~/Library/Application Support/Enso` on macOS, `%LOCALAPPDATA%/Enso` on Windows, and `$XDG_DATA_HOME/Enso` or `~/.local/share/Enso` on Linux). It auto-saves like any document. A separate first-launch marker prevents repeated opening or recreation, and existing sample edits are preserved. Explicit file/folder arguments take priority.

- Native dialogs for opening individual UTF-8 text files or folders.
- Lazy folder browsing, nested directory expansion, and manual refresh.
- New files in the workspace root. Without a folder, choose the file location when creating a note.
- Document tabs retain edits while switching files or folders. Hover a tab to smoothly expand its pill and fade in the close icon, with no reserved close-button space at rest; Ctrl W (or Cmd W on macOS) closes the current file after saving pending edits.
- Draggable dividers resize the sidebar and the editor/preview split. The sidebar animates open/closed while preserving its resized width and respecting reduced-motion settings. Toolbar actions use icons with tooltips.
- macOS uses a compact custom titlebar with native traffic lights, Enso branding, and a sidebar collapse/expand button. File tabs share the macOS titlebar, including their animated hover close buttons; there is no separate tab row. Windows and Linux keep their native titlebars.
- Read and Split reveal the section containing the editor cursor when switching views. In Split, editor clicks and cursor movement also move the preview to the corresponding passage.
- A compact 20 px status bar shows a word count; counting runs on background snapshots and debounces while typing.
- Markdown files above 256 KiB use a virtual section reader: only visible sections are parsed and laid out, with source editing initialized on demand outside the render pass. Large-file editing disables syntax highlighting, code folding, auto-close, and soft wrapping.
- Markdown reading, source editing, and live split preview using GPUI Kit's renderer.
- Plain text files open directly in the editor.
- Automatic saving after 500 ms of idle time, serialized background writes, external-change detection, and pending-save flushing when quitting. Cmd/Ctrl S saves immediately.
- Relative local images resolve against the document folder. Web links open in the browser; relative file links reveal the target in the system file manager.

Use Cmd on macOS and Ctrl on Windows/Linux: O opens a file, Shift O opens a folder, N creates a file, W closes the current file after saving pending edits, S saves, E toggles Edit/Read, backslash toggles Split (press again to return to the previous view), and Q quits. Ctrl+E and Ctrl+backslash also work on macOS; Split no longer requires Shift.

## Keyboard shortcuts

⌘ on macOS, Ctrl on Windows and Linux.

| App | |
|---|---|
| ⌘O / ⌘⇧O | Open file / open folder as a workspace |
| ⌘N / Ctrl⇧N | New file / new folder in the selected folder |
| Enter (F2 on Windows/Linux) | Rename the selected file or folder (or double-click its name) |
| ⌘⌫ (Delete on Windows/Linux) | Move the selected file or folder to the Trash, after confirming |
| ⌘S | Save now (saving is also automatic) |
| ⌘W | Close the current file |
| ⌘E | Switch between Read and Edit |
| ⌘\\ | Split view |
| ⌘, | Settings |
| ⌘Q | Quit |

In the Markdown editor:

| Formatting | |
|---|---|
| ⌘B | Bold `**text**` |
| ⌘I | Italic `*text*` |
| ⌘⇧X | Strikethrough `~~text~~` |
| ⌘⇧C | Inline code `` `text` `` |
| ⌘K | Link `[text](url)` |
| ⌘1 – ⌘6 | Heading level 1–6 |
| ⌘0 | Back to body text |
| ⌘⇧8 | Bullet list |
| ⌘⇧7 | Numbered list |
| ⌘⇧L | Task list `- [ ]` |
| ⌘Enter | Tick / untick a task |
| ⌘⇧. | Quote |
| ⌘⌥C | Code block |

Each applies to the selection, or to the word or line at the cursor; using it again removes the format. In lists, task lists and quotes, Enter continues with the next item (an empty item ends the list, or steps a nested one out a level), and Tab / Shift-Tab nest and un-nest items. The editor's own shortcuts still apply: ⌘Z / ⌘⇧Z undo and redo, ⌘F find, ⌘⇧F replace, ⌘] / ⌘[ indent and outdent lines, ⌥⌘↑ / ⌥⌘↓ add cursors.

## Platforms

One codebase targets macOS, Windows, and Linux using GPUI Kit's native backends. This initial version is developed on macOS; Windows and Linux runtime verification remains to be done. Linux needs GPUI's native build dependencies (including a C/C++ toolchain, pkg-config, fontconfig, Wayland/X11 development libraries, and a working Vulkan driver). Follow the platform prerequisites in the supplied GPUI Kit repository. Distribution installers and OS file associations are future work.

`vendor/gpui-pre-macos` is the Apache-2.0 upstream 0.3.10 crate with three `Option::map_or_default` calls replaced by stable `map_or` equivalents. This avoids an upstream stable-Rust compilation error without modifying the GPUI Kit checkout. See its `ENSO-PATCH.md`.

## Verification

```sh
cargo fmt --check
cargo check --locked
cargo test --locked
cargo test --features ui-tests repeated_shortcuts
```

The view-switch regression test uses the GPUI core harness to repeatedly leave a deeply scrolled editor, return to reading, toggle Split, and verify focus and source integrity. Read and Edit use separate element identities; only Split owns resizable panel state.

The reader tests verify lossless section indexing, bounded parser input, huge Unicode lines, and code-fence continuations. To profile a local file, run `ENSO_BENCH_FILE=/path/to/file.md cargo test profile_large_file -- --ignored --nocapture`.

The file lifecycle test covers Unicode text, saving, conflict detection, duplicate creation, path traversal rejection, and binary-file rejection.

## Current scope

Text input is UTF-8; binary and other encodings show an error. Workspace symlinks are skipped to avoid cycles. Changes auto-save to the original file. Workspace state is retained only while Enso is running. Closing the last file leaves the window open. Save failures keep the file open. Reading, section indexing, and disk writes run on background workers. The large-file reader retains the complete source and indexes UTF-8 byte ranges without parsing a full-document AST. It prefers paragraph boundaries and carries fenced code blocks across sections. Very long paragraphs, lists, tables, and reference definitions spanning sections can render differently from a whole-document parse; cross-section text selection is not yet supported. No file watcher, search across notes, plugins, or vault database yet.
