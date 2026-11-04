<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/logo-light.png">
    <img src="assets/logo-dark.png" alt="enso logo" width="128">
  </picture>
</p>

<h1 align="center">enso</h1>

<p align="center">A native, reading-first Markdown workspace built in Rust with GPUI Kit. No webview or Tauri.</p>

## Run

Install stable Rust (1.95 or later). GPUI Kit comes from crates.io (`gpui-kit = "0.8.1"`), so a plain clone builds on its own.

```sh
cargo run
cargo run -- /absolute/path/to/note.md
cargo run -- /absolute/path/to/folder
```

On macOS, `make` lists the build targets. `make build.macos` produces the release: an optimized `target/enso.app` with its Quick Look extension (Space-bar previews of Markdown in Finder), ad-hoc signed, and `target/enso-<version>-<arch>.dmg`. `make app` stops at the app, `make app.debug` makes a quick unoptimized one, and `make install` copies it to `/Applications` and registers it and its Quick Look preview. The bundle uses `assets/enso.icns`; `assets/enso.png` also supplies the Dock icon during `cargo run`.

The initial build downloads and compiles GPUI's native dependencies. Subsequent builds are incremental. Development builds optimize GPUI Base, Ropey, and the text index to avoid slow million-line editor initialization under `cargo run`; the first rebuild after a profile change takes longer.

## First version

On the first launch without a file/folder argument, enso creates and opens an editable `Get Started.md` in its local application data folder (`~/Library/Application Support/enso` on macOS, `%LOCALAPPDATA%/enso` on Windows, and `$XDG_DATA_HOME/enso` or `~/.local/share/enso` on Linux). It auto-saves like any document. A separate first-launch marker prevents repeated opening or recreation, and existing sample edits are preserved. Explicit file/folder arguments take priority.

- Native dialogs for opening individual UTF-8 text files or folders.
- Lazy folder browsing, nested directory expansion, and manual refresh.
- New files in the workspace root. Without a folder, choose the file location when creating a note.
- Document tabs retain edits while switching files or folders. Hover a tab to smoothly expand its pill and fade in the close icon, with no reserved close-button space at rest; Ctrl W (or Cmd W on macOS) closes the current file after saving pending edits.
- Draggable dividers resize the sidebar and the editor/preview split. The sidebar animates open/closed while preserving its resized width and respecting reduced-motion settings. Toolbar actions use icons with tooltips.
- macOS uses a compact custom titlebar with native traffic lights, enso branding, and a sidebar collapse/expand button. File tabs share the macOS titlebar, including their animated hover close buttons; there is no separate tab row. Windows and Linux keep their native titlebars.
- Read and Split reveal the section containing the editor cursor when switching views. In Split, editor clicks and cursor movement also move the preview to the corresponding passage.
- A compact 20 px status bar shows a word count; counting runs on background snapshots and debounces while typing.
- Markdown files above 256 KiB use a virtual section reader: only visible sections are parsed and laid out, with source editing initialized on demand outside the render pass. Large-file editing disables syntax highlighting, code folding, auto-close, and soft wrapping.
- Markdown reading, source editing, and live split preview using GPUI Kit's renderer.
- Plain text files open directly in the editor.
- Automatic saving after 500 ms of idle time, serialized background writes, external-change detection, and pending-save flushing when quitting. Cmd/Ctrl S saves immediately.
- Relative local images resolve against the document folder. Web links open in the browser; relative file links reveal the target in the system file manager.

Use Cmd on macOS and Ctrl on Windows/Linux: O opens a file, Shift O opens a folder, N creates a file, W closes the current file after saving pending edits, S saves, E toggles Edit/Read, backslash toggles Split (press again to return to the previous view), and Q quits. Ctrl+E and Ctrl+backslash also work on macOS; Split no longer requires Shift.

## Releases

`.github/workflows/release.yml` builds release packages on GitHub Actions:

- **macOS:** disk images for Apple silicon (`arm64`) and Intel (`x64`), with the Quick Look extension. Both are built on Apple silicon; `ENSO_TARGET=x86_64-apple-darwin ./scripts/build-mac.sh` makes the Intel one locally.
- **Windows:** for x64 and ARM64, an installer (`-setup.exe`, made with Inno Setup from `installer/windows/enso.iss`) that adds enso to the Start menu and to Open With for Markdown files, and a portable zip.
- **Linux:** for x64 and ARM64, an AppImage, a `.deb` (Debian, Ubuntu), an `.rpm` (Fedora, openSUSE) and a `.tar.gz`, all made by `scripts/package-linux.sh`. They're built on Ubuntu 22.04, so they run on distros with glibc 2.35 or later.

Run it by hand from the Actions tab (Release -> Run workflow): it puts the packages in a draft GitHub Release for the version in `Cargo.toml`, to check and publish from the Releases page. Running it again for the same version replaces the draft's files.

The macOS app is ad-hoc signed, not notarized, so Gatekeeper asks on first open; Windows SmartScreen asks too, as the installer isn't code-signed. On Linux, the `.tar.gz` works on any distro: unpack it (`tar -xzf enso-<version>-linux-<arch>.tar.gz`) and run `./install.sh` (or `sh install.sh`) to add enso to the app launcher for your user. The AppImage needs FUSE 2 (`libfuse2`) on some distros, such as Ubuntu 24.04.

## Reading controls

- **Settings → Appearance → Font Style** offers the current default, Inter, Lora, JetBrains Mono, Poppins and Fira Code. The fonts are bundled for offline use. The document’s **⋯** menu also offers these fonts and a small-to-large size slider with a live preview and Reset. Reading preferences apply to every file/window and are remembered; application controls and the source editor keep their own typography.
- **Export → Font Style** starts with your reading font and lets you choose a different font for PDF or HTML without changing the reader. PDF embeds the selected font. HTML exports as `Note Name/index.html`, with styles in `public/style.css`, local images in `public/images/`, and the selected bundled fonts and license in `public/fonts/`. Open `index.html` or share the whole folder. Choose a new folder name if one already exists; existing folders are preserved.
- Hover an image to drag its side handles and resize it while keeping its aspect ratio. The hover toolbar offers left (default), center, and right alignment. Its Edit button opens alt text and image replacement controls; replacements are copied into `.assets/images`.
- Click **Edit table** below a table to edit its cells. Tab and Shift+Tab move between cells; hover a cell and click its three-dot menu to add or remove rows and columns. Apply saves one Markdown edit; Cancel leaves the source unchanged.
- Click task checkboxes in Read or Split view to check or uncheck them without changing views.
- The slim outline rail sits to the left of the document. Marks near the pointer widen smoothly, a nearby card previews the section, and clicking jumps there. Heading indexing runs in the background. **Settings → Editor → Document Outline** toggles the rail; it is enabled by default.

**Settings → Editor → Spell Check** is enabled by default. Misspellings in Markdown prose have a wavy underline; right-click for spelling suggestions. Checks run locally after a typing pause, using macOS’s spelling service or the Windows Spell Checking API. Linux uses Enchant 2 and an installed dictionary (for example, `libenchant-2-2` and `hunspell-en-us` on Debian/Ubuntu). Code and link destinations are excluded. Spell check is automatically disabled for files of 2.5 MiB or larger, without changing the global setting. No document text is sent to a web service.

## Command palette

Press **Ctrl+Shift+P** (or **Cmd+Shift+P** on macOS), or choose **File → Command Palette…**. Search by name or everyday words such as “todo”, “photo” or “line”; use ↑/↓ and Enter, or click a command. Escape or clicking outside closes it and returns focus to where you were.

In a Markdown file's Edit or Split view, **Insert** offers links, images, checkboxes, lists, tables, code blocks, quotes, dividers, footnotes, highlights and today's date. **Format** offers bold, italic, strikethrough, inline code, headings and body text. Formatting uses the selection you had before opening the palette. Highlight opens preset swatches and a custom color picker; to recolor an existing highlight, place the cursor inside it and run Highlight again. Images are copied into the workspace's hidden `.assets/images` folder; Table asks for data rows and columns (1–20 each) and adds a header row.

**Files** searches filenames and relative paths throughout the current workspace, including collapsed folders. Selecting a result switches to its existing tab or opens it. Hidden files and folders and symlinks are excluded.

**App** and **Workspaces** offer new files and folders, recent workspaces, view modes, swapping split sides, Settings and showing the current file in the system file manager. Insert and Format are hidden in Read view and for plain-text files.

Type `/` on an empty Markdown line in Edit or Split to open the Insert menu beside the cursor. Select text and press **Cmd+/** on macOS (**Ctrl+/** on Windows/Linux) to open the same menu and apply Link, Highlight, or another Insert command to the selection. Choosing a command after typing `/` replaces the slash; Escape leaves it as text. Recently used commands appear first and are remembered across sessions.

## Keyboard shortcuts

⌘ on macOS, Ctrl on Windows and Linux.

| App | |
|---|---|
| ⌘O / ⌘⇧O | Open file / open folder as a workspace |
| ⌘N / ⌘⇧N | New file / new folder in the selected folder |
| Enter (F2 on Windows/Linux) | Rename the selected file or folder (or double-click its name) |
| ⌘⌫ (Delete on Windows/Linux) | Delete the selected file or folder (to the Trash / Recycle Bin; only a permanent delete asks first) |
| ⌘⇧P / Ctrl⇧P | Command palette |
| ⌘B | Collapse or expand the sidebar, remembered for every window (Bold while typing in a Markdown file) |
| ⌘S | Save now (saving is also automatic) |
| ⌘W | Close the current file |
| ⌘⇧T / Ctrl⇧T | Reopen the last closed tab; repeat for earlier tabs |
| ⌘E | Switch between Read and Edit |
| ⌘\\ | Split view |
| ⌘⇧\\ | Swap editor and reader in Split view |
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
| ⌘/ | Insert popover for the selection (Ctrl+/ on Windows/Linux) |
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

One codebase targets macOS, Windows, and Linux using GPUI Kit's native backends. This initial version is developed on macOS; Windows and Linux runtime verification remains to be done. Linux needs GPUI's native build dependencies (including a C/C++ toolchain, pkg-config, fontconfig, Wayland/X11 development libraries, and a working Vulkan driver). Follow the platform prerequisites in the supplied GPUI Kit repository.

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

Text input is UTF-8; binary and other encodings show an error. Workspace symlinks are skipped to avoid cycles. Changes auto-save to the original file. Workspace state is retained only while enso is running. Closing the last file leaves the window open. Save failures keep the file open. Reading, section indexing, and disk writes run on background workers. The large-file reader retains the complete source and indexes UTF-8 byte ranges without parsing a full-document AST. It prefers paragraph boundaries and carries fenced code blocks across sections. Very long paragraphs, lists, tables, and reference definitions spanning sections can render differently from a whole-document parse; cross-section text selection is not yet supported. No file watcher, search across notes, plugins, or vault database yet.
