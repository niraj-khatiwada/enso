# Changelog

### 1.3.0

Features:

- **Live Preview**: Edit right on the page, as in Notion: the Markdown is never shown. On by default (Settings › Editor › Live Preview); off, Edit, Read and Split are as before. The note stays plain Markdown underneath: each keystroke changes only the text you're editing, so everything else keeps its exact bytes, and undo, saving and sync work on the file itself.
  - Paragraphs and headings with bold, italic, strikethrough, inline code, links and wiki links, edited as drawn; `# ` and `- [ ] ` typed at a line's start become a heading and a checkbox.
  - Bullet, numbered and checkbox lists: Enter for the next item, Enter on an empty one to leave the list, Tab and Shift + Tab to indent, and checkboxes that tick.
  - Quotes, code blocks with highlighting, and tables, each cell edited in place: Tab and Enter move between cells and add rows.
  - Selection by drag, Shift, double and triple click; Cmd/Ctrl + A selects the line, then the page. Copy gives plain text (Cmd/Ctrl + Shift + C the Markdown); a copy pasted back keeps its formatting.
  - The editing keys of the system (word and line moves and deletes) and the editor's formatting keys: bold, italic, strikethrough, inline code (Cmd/Ctrl + E), links, headings, lists, checkboxes, quote and code block.
  - Links open on click; resting the pointer on one, or Cmd/Ctrl + K, shows a card to edit its text and address.
  - Backspace or Delete in an empty block removes it; the slash menu works on an empty line.
  - Tables like Notion's: each cell is its own field, the arrow keys stay in it, a click anywhere in a cell lands there, and + bars under and beside the table add a row or column. Hovering a cell shows its column's alignment and a ⋯ menu to insert, align and delete rows and columns.
  - Inline HTML formatting (`<b>`, `<u>`, `<mark>`…), entities, reference links, footnotes and inline math are edited in place; a footnote mark or a formula moves and deletes as one, and Cmd/Ctrl + K edits a formula's TeX.
  - Hovering a highlight shows its colours and a colour picker. Text can be coloured too (Cmd/Ctrl + / › Color › Text Color, with a custom colour picker), written as `<span style="color: …">`.
  - A click on an image (or a diagram, a rule…) selects it; Backspace or Delete removes it, and Enter starts a new line under it.
  - Enter on an empty line adds another, as in Notion: extra blank lines show as empty lines to click into, type on and move through with the arrow keys.
  - Mermaid diagrams, block math, HTML blocks and callouts have an Edit button: their Markdown on the left, drawn live on the right as it's typed; undo and redo stay in it.
  - Images inside a sentence (Markdown or `<img>`) are drawn in the line, which stays editable around them, resized by their handles or Cmd/Ctrl + /, with the same toolbar as other images; callouts are drawn as callouts.
  - **Cmd/Ctrl + Shift + E** shows the block at the caret as its Markdown, to edit as text; again, Escape or moving away draws it again.
  - Blocks inside blocks are edited in place too: lists in quotes, and code, quotes and further paragraphs in list items; Enter keeps each line in its container.
  - **Cmd/Ctrl + /** opens actions for what the caret is in, with Convert To, Color and Case as menus of their own: formatting buttons (bold, italic, underline, strikethrough, code, link, highlight, math, clear formatting; written as Markdown or HTML alike), highlight colours, letter case, Convert To, Mark as Done, table rows and columns, and Duplicate, Delete or Edit as Markdown for the block.
- The outline's section previews read as text in Live Preview, without Markdown.
- Cmd/Ctrl + = and − make the reading text larger and smaller, as the ⋯ menu's size slider does; Cmd/Ctrl + 0 resets it.
- Find (Cmd/Ctrl + F) highlights its matches on the live page and scrolls to each; a search result opened flashes its match there; and a heading picked in the outline puts the caret on it.

### 1.2.0

Features:

- **About Settings**: A new About section shows enso’s version, a manual update check, and links to the changelog, website, GitHub and Code For Real. Update checks show whether a newer release is available, the app is current, or the request failed.

Enhancements:

- Automatic update checking now lives in Settings › About. Manual checks work even when automatic checks are off; nothing downloads or installs on its own.
- Removed the General section. Settings opens to Appearance, and Confirm Before Deleting Files & Folders now lives in Editor.
- First launch opens the clean empty view without creating or opening a Get Started note.
- Download buttons on the website show file extensions without package sizes, including in tooltips.
- The About tagline and website copy use lowercase “markdown” consistently.

### 1.1.0

Features:

- **Pinned Notes**: Pin a tab from its right-click menu, the document's **⋯** menu or the command palette. Pinned tabs stay first, show a pin icon, and reopen pinned with the workspace. Unpin a tab before closing it; Cmd/Ctrl + W and the close button cannot remove pinned tabs.
- **Wiki Links and Backlinks**: Type `[[` to find another note in the workspace, select it with the keyboard or mouse, and click the link in Read view to open it. A **Linked from** list at the end of each note shows its incoming links. New links include the file extension; existing extensionless links and custom labels remain supported.
- **Callouts**: GitHub-style Note, Tip, Important, Warning and Caution blocks render as calm, tinted panels. Obsidian-style custom titles are supported, and the panels carry into HTML and PDF exports.
- **Math**: Inline `$…$` and display `$$…$$` equations render natively and offline, including fractions, roots, sums, integrals and matrices. HTML exports contain self-contained SVG, and PDFs draw equations as sharp vector paths, even with image export disabled.

Enhancements:

- Ctrl + Tab moves to the next tab and Ctrl + Shift + Tab to the previous one, on every platform.
- On macOS, every shortcut uses Cmd, and Control keys are left to text editing (Ctrl + E moves to the end of the line again). Tabs still cycle with Ctrl + Tab, as in browsers.
- Backspace in the insert menu with nothing typed closes it and puts the cursor back where it was; opened with `/`, the slash is erased too.
- Copy a file or folder in the sidebar with Cmd/Ctrl + C and paste it into any folder with Cmd/Ctrl + V, while the sidebar has focus. On macOS, copied files paste in Finder too. A folder can be pasted into itself, as a copy inside it. Cmd/Ctrl + X cuts instead, and pasting moves the item.
- Select several files and folders in the sidebar with Cmd + click (Ctrl + click on Windows and Linux), or Shift + click for a range within one folder; copy, cut, drag and delete them together.
- When no command matches in the command palette, Search Notes for what you typed is offered; press Enter and the same words search every note.
- Search everything finds text in large notes too: files over 4 MB are searched straight from disk, without slowing the search down.
- Find in Read view: Cmd/Ctrl + F opens a find bar over the page, highlights every match, and steps through them with Enter and Shift + Enter. The editor keeps its own Find and Replace.
- Opening a search result selects the match in the editor and highlights it in the reader for a few seconds, so it's easy to spot.
- Focus Mode (Cmd/Ctrl + Shift + Enter, the command palette, or the document's **⋯** menu): the sidebar, tabs, outline and word count step away and the page sits in a centred column. While writing, the line with the cursor stays in the middle of the view and the other paragraphs fade. Escape leaves it.
- Pasting a web address over selected words turns them into a link.
- Moving a file into a folder that already has one by that name keeps both, numbering the moved one, instead of refusing.
- F2 renames the selected file or folder on every platform; Enter still works on macOS.
- The word count sits in a compact pill at the bottom-right instead of a full-width status bar, giving the document more room.
- Large-document reading keeps bounded equations and callouts together instead of splitting them between rendered sections.

Bug Fixes:

- Moving or copying notes rebases relative image and file paths; renaming a note updates links that point to it.
- The workspace watcher checks arriving and changed notes regardless of whether they came through enso, a file manager, or sync. Broken local paths are repaired when a unique target can be identified; ambiguous matches are left unchanged.
- Link repairs preserve unsaved edits and leave code examples, web addresses and footnote text intact. Unchanged large notes are not repeatedly scanned.

### 1.0.0

Features:

- **Export**: Export a note to PDF, HTML or Rich Text from the **⋯** menu, the command palette or the File menu, with or without its images. PDF is laid out by enso itself, in your reading font, with page numbers, links and bookmarks, and a choice of page size and orientation. HTML is a portable folder with its styles, images and fonts. Rich text pastes cleanly into mail and documents.
- **Mermaid Diagrams**: Mermaid code blocks are drawn as diagrams in the reader, and carried into every export. Insert one from the command palette or the slash menu.
- **Reading Fonts**: Read in the default font, Inter, Lora, JetBrains Mono, Poppins or Fira Code, at the size you like. The fonts are bundled for offline use, and the choice is remembered for every file and window. Exports can use a different font.
- **Spell Check**: Misspellings in your prose are underlined, with suggestions on right-click. Checks run on your computer with the system's own spelling service; no text is sent anywhere.
- **Command Palette**: Every action in one search (Cmd/Ctrl + Shift + P), found by name or everyday words such as “todo” or “photo”. Insert links, images, tables, tasks, footnotes, highlights and more, format text, and open any file in the workspace.
- **Slash Menu**: Type `/` on an empty line, or select text and press Cmd/Ctrl + /, to insert or format right where you are.
- **Search Everything**: Search inside every note of the workspace from the command palette (Cmd/Ctrl + Shift + S).
- **Document Outline**: A slim rail of headings beside the page. Hover it to preview a section, and click to go there.
- **Highlights**: Highlight text with preset swatches or any color you pick.
- **Pictures**: Web images load in the reader, and can be turned off in Settings. Resize and align images from their hover toolbar, and view full size, copy or save any image or diagram.
- **Lock Page**: Lock a finished note so it can't be changed from the editor or the reader. It stays locked across restarts and renames.
- **Sessions**: Workspaces reopen with their tabs, active tab, view, sidebar and expanded folders.
- **Watched Folders**: The sidebar follows files added, removed or renamed outside enso, and open files changed on disk reload quietly, or ask when you have unsaved edits.
- **Drag and Drop**: Move files and folders in the sidebar, drop or paste files from Finder, drop images on a note to insert them, and reorder tabs by dragging.
- **Reopen Closed Tabs**: Bring back the last tab you closed with Cmd/Ctrl + Shift + T, and keep pressing for earlier ones.
- **Update Notice**: enso checks for a new version once a day and shows a toast with a Download button. Turn it off in Settings › General › Check for Updates.
- **More Packages**: An Intel disk image for macOS, an installer for Windows, and an AppImage, .deb and .rpm for Linux, for x64 and ARM64.

Enhancements:

- Cmd/Ctrl + B collapses or expands the sidebar, remembered for every window and session.
- In Split, clicking or selecting in the reader moves the editor's cursor there.
- Swap the editor and reader sides of Split from the divider, or with Cmd/Ctrl + Shift + \.
- Edit Table also works for simple HTML tables.
- Confirm Before Deleting Files & Folders, on by default, asks before anything goes to the Trash.
- Rename File and Delete File are in the command palette.
- The sidebar selects and reveals the file you switch to.
- Short messages appear as compact toasts, with Show in Finder after an export.
- The welcome screen offers Open Workspace, Open File and Command Palette.

Bug Fixes:

- Fixed Windows opening a console window behind enso.
- Fixed the Linux launcher icon sometimes showing as a generic icon.
- Fixed repeated Reopen Closed Tab presses going to a tab that wasn't shown.
- Fixed task checkboxes not lining up with their text.
- Fixed popups, the palette highlight and highlight swatches in dark mode.

### 0.1.0

Features:

- **Read, Edit and Split**: Read Markdown as a page, edit the source, or keep both side by side. Cmd/Ctrl + E switches between Read and Edit, and Cmd/Ctrl + \ opens Split.
- **Workspaces**: Open a folder as a workspace with a file tree, in as many windows as you like. Create, rename and move files and folders to the Trash from the sidebar.
- **Auto-Save**: Changes save on their own after a short pause, and when you quit.
- **Markdown Shortcuts**: Bold, italic, links, headings, lists, tasks, quotes and code blocks from the keyboard. Using one again removes it.
- **Large Files**: Big documents are read section by section, so they open fast and stay smooth.
- **Quick Look**: Press Space on a Markdown file in Finder to read it (macOS).
- **Tasks**: Tick task checkboxes right in the reader.
- **Images**: Paste images into a note, and see relative images beside your notes.
- **Every Desktop**: Packages for macOS (Apple silicon), Windows (x64 and ARM64) and Linux (x64 and ARM64).

Bug Fixes:

- Fixed renaming being rolled back on Enter.
- Fixed long file names in tabs; they now end in “…” at a fixed width.
- Fixed the Linux package failing to install when unpacked without executable bits.
- Fixed the header on Linux stretching to half the window.
- Fixed Windows crashing on start because of a small main-thread stack.
