# Changelog

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
