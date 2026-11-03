//! Exporting a note: PDF (typeset by Typst), HTML (a page with an images
//! folder beside it), and rich text on the clipboard.

pub mod html;
pub mod pdf;

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Pdf,
    Html,
    /// Copied to the clipboard rather than saved.
    RichText,
}

impl Format {
    const ALL: [Self; 3] = [Self::Pdf, Self::Html, Self::RichText];

    fn extension(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Html => "html",
            Self::RichText => "",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Pdf => "PDF",
            Self::Html => "HTML",
            Self::RichText => "Rich Text",
        }
    }

    fn icon(self) -> IconName {
        match self {
            Self::Pdf => IconName::FileText,
            Self::Html => IconName::Globe,
            Self::RichText => IconName::ClipboardCopy,
        }
    }
}

/// The Export dialog's choices. The last ones used are offered again.
#[derive(Clone)]
pub struct ExportDialog {
    pub(crate) choice: ExportChoice,
    focus: FocusHandle,
}

/// What the Export dialog offers, remembered between exports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExportChoice {
    pub format: Format,
    pub images: bool,
    pub paper: pdf::Paper,
    pub landscape: bool,
}

impl Default for ExportChoice {
    fn default() -> Self {
        Self { format: Format::Pdf, images: true, paper: pdf::Paper::local(), landscape: false }
    }
}

impl Enso {
    /// The active note's text, its folder (for images) and its title.
    fn export_source(&self, cx: &App) -> Option<(String, Option<PathBuf>, String)> {
        let doc = self.active.map(|i| &self.documents[i]).filter(|d| d.markdown)?;
        let text = if doc.editor_loaded {
            doc.editor.read(cx).value().to_string()
        } else {
            doc.saved.as_ref().clone()
        };
        let base = doc.path.as_ref().and_then(|p| p.parent()).map(PathBuf::from);
        Some((text, base, self.export_title()?))
    }

    /// The active note's name, without its extension.
    fn export_title(&self) -> Option<String> {
        let doc = self.active.map(|i| &self.documents[i]).filter(|d| d.markdown)?;
        Some(
            doc.path
                .as_ref()
                .and_then(|p| p.file_stem())
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Untitled".into()),
        )
    }

    /// Open the Export dialog for the active note.
    pub(crate) fn open_export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.export_source(cx).is_none() {
            self.message("Open a note to export it.", true, cx);
            return;
        }
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        self.closing_export = None;
        self.export_dialog = Some(ExportDialog { choice: self.export_choice, focus });
        cx.notify();
    }

    pub(crate) fn close_export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(dialog) = self.export_dialog.take() {
            self.closing_export = Some(dialog);
            self.focus.focus(window, cx);
            cx.notify();
        }
    }

    /// Export (or copy) with the dialog's choices.
    fn run_export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = self.export_dialog.as_ref() else {
            return;
        };
        let choice = dialog.choice;
        self.export_choice = choice;
        self.close_export(window, cx);
        match choice.format {
            Format::RichText => self.copy_rich_text(choice.images, cx),
            _ => self.export_note(choice, window, cx),
        }
    }

    /// Ask where to save the active note as `format`, then write it there.
    fn export_note(&mut self, choice: ExportChoice, window: &mut Window, cx: &mut Context<Self>) {
        let format = choice.format;
        let images = choice.images;
        let Some((source, base, title)) = self.export_source(cx) else {
            self.message("Open a note to export it.", true, cx);
            return;
        };
        let folder = base
            .clone()
            .or_else(dirs::document_dir)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let name = format!("{title}.{}", format.extension());
        let receiver = cx.prompt_for_new_path(&folder, Some(&name));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(target))) = receiver.await else {
                return;
            };
            let file = target
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let busy = this.update(cx, |this, cx| {
                this.busy(format!("export:{}", target.display()), format!("Exporting “{file}”…"), cx)
            });
            let Ok(busy) = busy else {
                return;
            };
            let exported = target.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    match format {
                        Format::Pdf => pdf::pdf(
                            &source,
                            base.as_deref(),
                            &title,
                            pdf::PdfLayout { paper: choice.paper, landscape: choice.landscape, images },
                        )
                            .and_then(|bytes| std::fs::write(&target, bytes).map_err(|e| e.to_string())),
                        Format::Html => html::export(&source, base.as_deref(), &title, &target, images)
                            .map_err(|e| e.to_string()),
                        Format::RichText => Ok(()),
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(()) => {
                    // Replaces the "Exporting…" toast, with a way to the file.
                    let reveal = if cfg!(target_os = "macos") { "Show in Finder" } else { "Show in File Explorer" };
                    let note = ui::toast::done(busy.id.clone(), "Exported", file)
                        .action(IconName::FolderOpen, reveal, move |_, cx| cx.reveal_path(&exported));
                    this.finish(busy, Some(note), cx);
                }
                Err(error) => {
                    this.finish(busy, None, cx);
                    this.message(format!("Couldn't export “{file}”: {error}"), true, cx);
                }
            });
        })
        .detach();
    }

    /// Put the active note on the clipboard as formatted text (images
    /// inline, if wanted), with its Markdown as the plain-text version, so it
    /// pastes with its formatting into Mail, Docs, Word or Notion.
    pub(crate) fn copy_rich_text(&mut self, images: bool, cx: &mut Context<Self>) {
        let Some((source, base, title)) = self.export_source(cx) else {
            self.message("Open a note to copy it.", true, cx);
            return;
        };
        // Reading and scaling images takes a moment; it's done off the
        // window's thread, with a spinner if it runs long.
        let busy = self.busy("copy-rich-text", format!("Copying “{title}”…"), cx);
        let task = cx.background_executor().spawn(async move {
            let mut mode = if images {
                html::Images::Inline { used: 0 }
            } else {
                html::Images::None
            };
            (html::body(&source, base.as_deref(), &mut mode), html::plain_text(&source))
        });
        cx.spawn(async move |this, cx| {
            let (body, plain) = task.await;
            let _ = this.update(cx, |this, cx| match set_clipboard_html(&body, &plain) {
                Ok(()) => {
                    let note = ui::toast::done(
                        busy.id.clone(),
                        "Copied as Rich Text",
                        format!("Paste “{title}” into Mail, Docs or Notion."),
                    );
                    this.finish(busy, Some(note), cx);
                }
                Err(error) => {
                    this.finish(busy, None, cx);
                    this.message(format!("Couldn't copy: {error}"), true, cx);
                }
            });
        })
        .detach();
    }

    /// PDF page size and orientation, as two rows of choices.
    fn page_options(&self, choice: &ExportChoice, theme: &gpui_kit::component::Theme, cx: &mut Context<Self>) -> Div {
        let segment = |id: SharedString, label: &'static str, selected: bool| {
            div()
                .id(id)
                .px_2p5()
                .py_1()
                .rounded_md()
                .text_sm()
                .cursor_pointer()
                .when(selected, |s| s.bg(theme.secondary).font_weight(FontWeight::MEDIUM))
                .when(!selected, |s| {
                    s.text_color(theme.muted_foreground)
                        .hover(|s| s.bg(theme.secondary.opacity(0.5)))
                })
                .child(label)
        };
        let row = |label: &'static str| {
            h_flex()
                .items_center()
                .justify_between()
                .child(div().text_sm().child(label))
        };
        v_flex()
            .gap_2()
            .child(row("Page Size").child(h_flex().gap_0p5().children(pdf::Paper::ALL.into_iter().map(|paper| {
                segment(format!("export-paper-{}", paper.label()).into(), paper.label(), choice.paper == paper)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(dialog) = this.export_dialog.as_mut() {
                            dialog.choice.paper = paper;
                            cx.notify();
                        }
                    }))
            }))))
            .child(row("Orientation").child(h_flex().gap_0p5().children([(false, "Portrait"), (true, "Landscape")].into_iter().map(
                |(landscape, label)| {
                    segment(format!("export-orientation-{label}").into(), label, choice.landscape == landscape)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(dialog) = this.export_dialog.as_mut() {
                                dialog.choice.landscape = landscape;
                                cx.notify();
                            }
                        }))
                },
            ))))
    }

    /// The Export dialog over a dimmed window, fading in and out.
    pub(crate) fn export_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let motion = cx.theme().motion_tokens();
        let progress = transition(
            "export-dialog-presence",
            if self.export_dialog.is_some() { 1.0_f32 } else { 0.0 },
            Transition::new(std::time::Duration::from_millis(180)).easing(motion.easing_move.clone()),
            window,
            cx,
        )
        .clamp(0., 1.);
        if self.export_dialog.is_none() && progress <= 0.001 {
            self.closing_export = None;
            return None;
        }
        let dialog = self.export_dialog.as_ref().or(self.closing_export.as_ref())?.clone();
        let theme = cx.theme().clone();
        let title = self.export_title().unwrap_or_default();
        let options = h_flex().gap_2().children(Format::ALL.into_iter().map(|format| {
            let selected = dialog.choice.format == format;
            v_flex()
                .id(SharedString::from(format!("export-format-{}", format.label())))
                // Equal widths whatever the label.
                .flex_1()
                .flex_basis(px(0.))
                .min_w_0()
                .gap_1p5()
                .p_3()
                .rounded_lg()
                .border_1()
                .cursor_pointer()
                .border_color(if selected { theme.ring } else { theme.border })
                .when(selected, |card| card.bg(theme.secondary))
                .when(!selected, |card| card.hover(|card| card.bg(theme.secondary.opacity(0.5))))
                .child(Icon::new(format.icon()).size_5().text_color(if selected {
                    theme.foreground
                } else {
                    theme.muted_foreground
                }))
                .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(format.label()))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(dialog) = this.export_dialog.as_mut() {
                        dialog.choice.format = format;
                        cx.notify();
                    }
                }))
        }));
        let images = h_flex()
            .gap_3()
            .items_center()
            .justify_between()
            .child(
                v_flex()
                    .gap_0p5()
                    .child(div().text_sm().child("Include Images"))
                    .child(div().text_xs().text_color(theme.muted_foreground).child(match dialog.choice.format {
                        Format::Html => "Copied into a folder beside the page",
                        Format::RichText => "Included where the app accepts images, like Mail or Docs",
                        Format::Pdf => "Fitted to the page",
                    })),
            )
            .child(
                gpui_kit::component::switch::Switch::new("export-images")
                    .checked(dialog.choice.images)
                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                        if let Some(dialog) = this.export_dialog.as_mut() {
                            dialog.choice.images = *checked;
                            cx.notify();
                        }
                    })),
            );
        let actions = h_flex()
            .justify_end()
            .gap_2()
            .child(ui::cancel_button("export-cancel").on_click(cx.listener(|this, _, w, cx| this.close_export(w, cx))))
            .child(
                ui::primary_button(
                    "export-run",
                    if dialog.choice.format == Format::RichText { "Copy" } else { "Export…" },
                )
                .on_click(cx.listener(|this, _, w, cx| this.run_export(w, cx))),
            );
        let panel = v_flex()
            .gap_4()
            .p_5()
            .child(
                v_flex()
                    .gap_0p5()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("Export"))
                    .child(div().text_sm().text_color(theme.muted_foreground).truncate().child(title)),
            )
            .child(options)
            .when(dialog.choice.format == Format::Pdf, |panel| {
                panel.child(self.page_options(&dialog.choice, &theme, cx))
            })
            .child(images)
            .child(actions);
        Some(
            div()
                .absolute()
                .inset_0()
                .occlude()
                .key_context("ExportDialog")
                .track_focus(&dialog.focus)
                .on_action(cx.listener(|this, _: &DismissExport, w, cx| this.close_export(w, cx)))
                .on_key_down(cx.listener(|this, event: &KeyDownEvent, w, cx| {
                    if event.keystroke.key == "enter" {
                        this.run_export(w, cx);
                    }
                }))
                .bg(black().opacity(ui::BACKDROP_OPACITY * progress))
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, w, cx| this.close_export(w, cx)))
                .child(
                    div()
                        .debug_selector(|| "export-dialog".into())
                        .w(px(460.))
                        .max_w(relative(0.9))
                        .relative()
                        .top(px(-8. * (1. - progress)))
                        .opacity(progress)
                        .modal_surface(&theme)
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(panel),
                )
                .into_any_element(),
        )
    }
}

/// HTML and its plain-text version on the system clipboard. One clipboard
/// handle lives for the whole run: on Linux it serves what was copied.
fn set_clipboard_html(html: &str, plain: &str) -> Result<(), String> {
    static CLIPBOARD: std::sync::Mutex<Option<arboard::Clipboard>> = std::sync::Mutex::new(None);
    let mut clipboard = CLIPBOARD.lock().map_err(|e| e.to_string())?;
    if clipboard.is_none() {
        *clipboard = Some(arboard::Clipboard::new().map_err(|e| e.to_string())?);
    }
    clipboard
        .as_mut()
        .expect("set above")
        .set_html(html, Some(plain))
        .map_err(|e| e.to_string())
}
