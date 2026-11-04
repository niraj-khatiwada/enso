//! GitHub alerts and Obsidian callouts, parsed once with shared semantics for
//! the native reader and both export formats.
use markdown::mdast::Node;
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}
impl Kind {
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value.to_ascii_lowercase().as_str() {
            "note" | "info" | "todo" => Self::Note,
            "tip" | "hint" | "success" | "check" | "done" => Self::Tip,
            "important" | "abstract" | "summary" | "example" => Self::Important,
            "warning" | "attention" | "question" | "help" => Self::Warning,
            "caution" | "danger" | "error" | "failure" | "bug" => Self::Caution,
            _ => return None,
        })
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Tip => "tip",
            Self::Important => "important",
            Self::Warning => "warning",
            Self::Caution => "caution",
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Note => "Note",
            Self::Tip => "Tip",
            Self::Important => "Important",
            Self::Warning => "Warning",
            Self::Caution => "Caution",
        }
    }
    pub fn accent(self, dark: bool) -> u32 {
        match (self, dark) {
            (Self::Note, false) => 0x0969da,
            (Self::Note, true) => 0x4493f8,
            (Self::Tip, false) => 0x1a7f37,
            (Self::Tip, true) => 0x3fb950,
            (Self::Important, false) => 0x8250df,
            (Self::Important, true) => 0xa371f7,
            (Self::Warning, false) => 0x9a6700,
            (Self::Warning, true) => 0xd29922,
            (Self::Caution, false) => 0xcf222e,
            (Self::Caution, true) => 0xf85149,
        }
    }
    pub fn rgb(self) -> (u8, u8, u8) {
        let n = self.accent(false);
        ((n >> 16) as u8, (n >> 8) as u8, n as u8)
    }
    pub fn tint(self) -> (u8, u8, u8) {
        let (r, g, b) = self.rgb();
        let mix = |v: u8| 255 - ((255 - v) as f32 * 0.07).round() as u8;
        (mix(r), mix(g), mix(b))
    }
}
#[derive(Clone)]
pub struct Callout {
    pub kind: Kind,
    pub title: String,
    pub children: Vec<Node>,
}
impl Callout {
    pub fn parse(node: &Node) -> Option<Self> {
        let Node::Blockquote(quote) = node else {
            return None;
        };
        let Node::Paragraph(paragraph) = quote.children.first()? else {
            return None;
        };
        let Node::Text(first) = paragraph.children.first()? else {
            return None;
        };
        let line = first.value.lines().next()?.trim();
        let content = line.strip_prefix("[!")?;
        let end = content.find(']')?;
        let kind = Kind::parse(&content[..end])?;
        let title = content[end + 1..].trim_start_matches(['+', '-']).trim();
        let title = if title.is_empty() {
            kind.title().into()
        } else {
            title.into()
        };
        let mut children = quote.children.clone();
        if let Node::Paragraph(p) = &mut children[0] {
            if let Node::Text(t) = &mut p.children[0] {
                t.value = t
                    .value
                    .split_once('\n')
                    .map_or("", |(_, rest)| rest)
                    .to_owned();
                if t.value.is_empty() {
                    p.children.remove(0);
                }
            }
            if p.children.is_empty() {
                children.remove(0);
            }
        }
        Some(Self {
            kind,
            title,
            children,
        })
    }
}

/// Strip one quote level and the marker line, recording every removed byte so
/// controls inside the panel still edit the correct original source ranges.
pub fn body_source(source: &str) -> (String, Vec<(Range<usize>, Range<usize>)>) {
    let mut text = String::new();
    let mut edits = Vec::new();
    let mut offset = 0;
    for (i, line) in source.split_inclusive('\n').enumerate() {
        let cut = if i == 0 {
            line.len()
        } else {
            let indent = line.bytes().take_while(|c| *c == b' ').count().min(3);
            if line.as_bytes().get(indent) == Some(&b'>') {
                indent + 1 + usize::from(line.as_bytes().get(indent + 1) == Some(&b' '))
            } else {
                0
            }
        };
        if cut > 0 {
            edits.push((text.len()..text.len(), offset..offset + cut));
        }
        text.push_str(&line[cut..]);
        offset += line.len();
    }
    (text, edits)
}

use crate::*;
use gpui_kit::component::text::{MarkdownNode, MarkdownParseContext, MarkdownPlugin};

#[derive(Clone)]
struct Panel {
    callout: Callout,
    body: String,
    mapping: Arc<crate::wiki::SourceMap>,
}
pub struct Plugin {
    pub owner: WeakEntity<Enso>,
    pub editor: EntityId,
    pub base: Option<PathBuf>,
    pub mapping: Arc<crate::wiki::SourceMap>,
}
impl MarkdownPlugin for Plugin {
    fn is_block(&self) -> bool {
        true
    }
    fn name(&self) -> &str {
        "enso-callout"
    }
    fn parse(&self, node: &Node, context: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let callout = Callout::parse(node)?;
        let source = context.node_source(node)?;
        let (body, edits) = body_source(source);
        let mapping = Arc::new(crate::wiki::SourceMap {
            offset: node.position()?.start.offset,
            prefix: 0,
            edits,
            parent: Some(self.mapping.clone()),
        });
        Some(
            MarkdownNode::new(
                self.name(),
                Panel {
                    callout,
                    body,
                    mapping,
                },
            )
            .text(node.to_string())
            .markdown(source.to_owned()),
        )
    }
    fn render(&self, node: &MarkdownNode, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let panel = node.data::<Panel>().unwrap();
        let theme = cx.theme().clone();
        let color: Hsla = rgb(panel.callout.kind.accent(theme.mode.is_dark())).into();
        let owner = self.owner.clone();
        let editor = self.editor;
        let image_base = self.base.clone();
        v_flex()
            .debug_selector({
                let kind = panel.callout.kind;
                move || format!("callout-{}", kind.name())
            })
            .w_full()
            .my_2()
            .px_4()
            .py_3()
            .gap_2()
            .rounded(px(10.))
            .bg(color.opacity(if theme.mode.is_dark() { 0.10 } else { 0.055 }))
            .border_l_2()
            .border_color(color.opacity(0.65))
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(color)
                    .child(
                        Icon::new(match panel.callout.kind {
                            Kind::Tip => IconName::Lightbulb,
                            Kind::Warning | Kind::Caution => IconName::TriangleAlert,
                            _ => IconName::Info,
                        })
                        .size_4(),
                    )
                    .child(panel.callout.title.clone()),
            )
            .child(
                crate::rich::decorate_mapped(
                    TextView::markdown(
                        ("callout-body", node.source_range().map_or(0, |r| r.start)),
                        panel.body.clone(),
                    ),
                    self.owner.clone(),
                    editor,
                    self.base.clone(),
                    panel.mapping.clone(),
                )
                .map(|view| crate::typography::style(view, cx))
                .selectable(true)
                .w_full()
                .image_source(move |uri| {
                    let value = uri.to_string();
                    if !value.contains(':')
                        && let Some(base) = &image_base
                    {
                        return ImageSource::from(base.join(crate::links::decode(&value)));
                    }
                    ImageSource::from(uri.clone())
                })
                .on_link_click(move |url, event, w, cx| {
                    if !event.is_right_click() {
                        let _ =
                            owner.update(cx, |app, cx| app.follow_note_link(editor, url, w, cx));
                    }
                }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{Callout, Kind, body_source};
    use ::core::prelude::v1::test;
    #[test]
    fn github_and_obsidian_markers_keep_body_formatting_and_source_offsets() {
        for (marker, kind) in [
            ("NOTE", Kind::Note),
            ("TIP", Kind::Tip),
            ("WARNING", Kind::Warning),
            ("important", Kind::Important),
            ("danger", Kind::Caution),
        ] {
            let source =
                format!("> [!{marker}] Custom title\n> A **formatted** body.\n>\n> - [ ] Task\n");
            let ast = markdown::to_mdast(&source, &markdown::ParseOptions::gfm()).unwrap();
            let callout = Callout::parse(&ast.children().unwrap()[0]).unwrap();
            assert_eq!(callout.kind, kind);
            assert_eq!(callout.title, "Custom title");
            assert!(!callout.children.is_empty());
            let (body, edits) = body_source(&source);
            assert_eq!(body, "A **formatted** body.\n\n- [ ] Task\n");
            let map = crate::wiki::SourceMap {
                offset: 100,
                prefix: 0,
                edits,
                parent: None,
            };
            let at = body.find("[ ]").unwrap();
            assert_eq!(map.point(at), Some(100 + source.find("[ ]").unwrap()));
        }
        let quote =
            markdown::to_mdast("> An ordinary quote", &markdown::ParseOptions::gfm()).unwrap();
        assert!(Callout::parse(&quote.children().unwrap()[0]).is_none());
    }
}

#[cfg(all(test, feature = "ui-tests"))]
mod ui_tests {
    use super::*;
    use ::core::prelude::v1::test;
    use gpui_kit::{Modifiers, TestAppContext, VisualTestContext, WindowOptions};
    #[gpui_kit::test]
    fn native_callouts_render_math_and_keep_checkbox_edits_on_original_source(
        cx: &mut TestAppContext,
    ) {
        let source =
            "> [!TIP]\n> - [ ] A task\n>\n> $$\n> \\frac{1}{2}\n> $$\n\nInline $x^2$ math.";
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::bind_view_keys(cx);
        });
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |w, cx| {
                cx.new(|cx| {
                    let mut app = Enso::new(w, cx);
                    app.add_document(None, source.into(), None, w, cx);
                    app
                })
            })
            .unwrap()
        });
        let draw = |cx: &mut TestAppContext| {
            cx.update_window(handle, |_, w, cx| {
                w.refresh();
                w.draw(cx).clear(cx);
            })
            .unwrap();
            cx.run_until_parked();
        };
        for _ in 0..5 {
            draw(cx);
        }
        let mut visual = VisualTestContext::from_window(handle, cx);
        assert!(visual.debug_bounds("callout-tip").is_some());
        assert!(visual.debug_bounds("math-block").is_some());
        let checkbox = visual
            .debug_bounds("task-checkbox-13")
            .expect("checkbox inside callout");
        visual.simulate_click(checkbox.center(), Modifiers::default());
        draw(cx);
        assert_eq!(
            cx.update(|cx| view.read(cx).documents[0]
                .editor
                .read(cx)
                .value()
                .to_string()),
            source.replace("[ ]", "[x]")
        );
    }
}
