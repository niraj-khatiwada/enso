//! Native controls for Markdown objects. Edits are committed as one source edit.
use super::*;
use gpui_kit::component::color_picker::{ColorPicker, ColorPickerState};
use gpui_kit::component::menu::DropdownMenu;
use gpui_kit::component::text::{
    MarkdownNode, MarkdownParseContext, MarkdownPlugin, markdown_ast::Node,
};
use std::ops::Range;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Alignment {
    #[default]
    Left,
    Center,
    Right,
}
impl Alignment {
    fn from_attributes(attrs: &HashMap<String, String>) -> Self {
        let style = attrs
            .get("style")
            .map(|s| s.to_ascii_lowercase().replace(' ', ""))
            .unwrap_or_default();
        if style.contains("margin-left:auto") && style.contains("margin-right:auto") {
            Self::Center
        } else if style.contains("margin-left:auto") {
            Self::Right
        } else {
            match attrs.get("align").map(String::as_str) {
                Some("center") => Self::Center,
                Some("right") => Self::Right,
                _ => Self::Left,
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Picture {
    pub(crate) src: String,
    pub(crate) alt: String,
    pub(crate) width: String,
    height: String,
    title: String,
    pub(crate) alignment: Alignment,
}
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
fn unescape(text: &str) -> String {
    text.replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}
fn attributes(tag: &str) -> HashMap<String, String> {
    let bytes = tag.as_bytes();
    let mut i = tag.find(char::is_whitespace).unwrap_or(tag.len());
    let mut result = HashMap::new();
    while i < bytes.len() {
        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b'/') {
            i += 1;
        }
        let start = i;
        while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-') {
            i += 1;
        }
        if start == i {
            break;
        }
        let name = tag[start..i].to_ascii_lowercase();
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if bytes.get(i) != Some(&b'=') {
            continue;
        }
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let quote = bytes.get(i).copied().filter(|b| *b == b'\'' || *b == b'"');
        if quote.is_some() {
            i += 1;
        }
        let start = i;
        while i < bytes.len()
            && if let Some(q) = quote {
                bytes[i] != q
            } else {
                !bytes[i].is_ascii_whitespace() && bytes[i] != b'>'
            }
        {
            i += 1;
        }
        result.insert(name, unescape(&tag[start..i]));
        if quote.is_some() {
            i += 1;
        }
    }
    result
}
pub(crate) fn html_picture(source: &str) -> Option<Picture> {
    let source = source.trim();
    if !source.to_ascii_lowercase().starts_with("<img ") || !source.ends_with('>') {
        return None;
    }
    // A single image only; never consume surrounding HTML.
    if source[..source.len() - 1].contains('>') {
        return None;
    }
    let mut attrs = attributes(source);
    let alignment = Alignment::from_attributes(&attrs);
    Some(Picture {
        src: attrs.remove("src")?,
        alt: attrs.remove("alt").unwrap_or_default(),
        width: attrs.remove("width").unwrap_or_else(|| "100%".into()),
        height: attrs.remove("height").unwrap_or_default(),
        title: attrs.remove("title").unwrap_or_default(),
        alignment,
    })
}
fn picture_node(node: &Node, source: &str) -> Option<Picture> {
    match node {
        Node::Image(image) => Some(Picture {
            src: image.url.clone(),
            alt: image.alt.clone(),
            width: "100%".into(),
            title: image.title.clone().unwrap_or_default(),
            ..Default::default()
        }),
        Node::Paragraph(p) if p.children.len() == 1 => picture_node(&p.children[0], source),
        Node::Html(_) => html_picture(source),
        _ => None,
    }
}
fn dimension(value: &str) -> Option<DefiniteLength> {
    let value = value.trim();
    if let Some(pct) = value.strip_suffix('%') {
        let n: f32 = pct.parse().ok()?;
        (n > 0. && n <= 100.).then(|| relative(n / 100.).into())
    } else {
        let n: f32 = value.trim_end_matches("px").parse().ok()?;
        (n.is_finite() && n > 0. && n <= 10000.).then(|| px(n).into())
    }
}
impl Picture {
    fn html(&self) -> String {
        let mut out = format!(
            "<img src=\"{}\" alt=\"{}\" width=\"{}\"",
            escape(&self.src),
            escape(&self.alt),
            escape(&self.width)
        );
        if !self.height.is_empty() {
            out.push_str(&format!(" height=\"{}\"", escape(&self.height)));
        }
        if !self.title.is_empty() {
            out.push_str(&format!(" title=\"{}\"", escape(&self.title)));
        }
        match self.alignment {
            Alignment::Left => {}
            Alignment::Center => {
                out.push_str(" style=\"display: block; margin-left: auto; margin-right: auto;\"")
            }
            Alignment::Right => {
                out.push_str(" style=\"display: block; margin-left: auto; margin-right: 0;\"")
            }
        }
        out.push_str(" />");
        out
    }
}

pub fn decorate(
    view: TextView,
    owner: WeakEntity<Enso>,
    editor_id: EntityId,
    base: Option<PathBuf>,
    source_offset: usize,
    prefix_len: usize,
) -> TextView {
    let inline = InlineImages {
        owner: owner.clone(),
        editor_id,
        base: base.clone(),
        offset: source_offset,
        prefix: prefix_len,
    };
    let image_owner = owner.clone();
    let table_owner = owner.clone();
    let task_owner = owner.clone();
    let task_base = base.clone();
    let diagram_owner = owner.clone();
    view.plugin(inline)
        .markdown_block_parser(|node, context| {
            let source = context.node_source(node)?;
            if let Node::Code(code) = node {
                if crate::diagram::is_mermaid(code.lang.as_deref()) {
                    return Some(
                        MarkdownNode::new("enso-mermaid", MermaidBlock { source: code.value.clone() })
                            .text(code.value.clone())
                            .markdown(source.to_owned()),
                    );
                }
            }
            if let Some(tasks) = parse_tasks(node, context) {
                return Some(
                    MarkdownNode::new("enso-tasks", tasks)
                        .text(source.to_owned())
                        .markdown(source.to_owned()),
                );
            }
            let picture = picture_node(node, source)?;
            Some(
                MarkdownNode::new("enso-image", picture.clone())
                    .text(picture.alt)
                    .markdown(source.to_owned()),
            )
        })
        .markdown_block_renderer("enso-mermaid", move |node, _, cx| {
            let block = node.data::<MermaidBlock>().unwrap();
            render_mermaid(&block.source, diagram_owner.clone(), cx)
        })
        .markdown_block_renderer("enso-image", move |node, window, cx| {
            render_picture(
                node,
                image_owner.clone(),
                editor_id,
                base.clone(),
                source_offset,
                prefix_len,
                window,
                cx,
            )
        })
        .markdown_block_renderer("enso-tasks", move |node, window, cx| {
            // A locked note's boxes show their state but don't change.
            let locked = task_owner.upgrade().is_some_and(|app| app.read(cx).is_locked(editor_id));
            let tasks = node.data::<Tasks>().unwrap();
            // Markers sit in a box one text line tall, centered on the item's
            // first line however tall the checkbox or the text is.
            let line_height = window.line_height();
            let mut list = v_flex().gap_1().w_full();
            for (i, item) in tasks.items.iter().enumerate() {
                let mut row = h_flex().items_start().gap_2().w_full();
                if let Some(checked) = item.checked {
                    let owner = task_owner.clone();
                    let marker = item
                        .marker
                        .and_then(|start| start.checked_sub(prefix_len))
                        .map(|start| start + source_offset);
                    row = row.child(
                        div()
                            .h(line_height)
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .debug_selector(move || {
                                format!("task-checkbox-{}", marker.unwrap_or(i))
                            })
                            .child(
                                gpui_kit::component::checkbox::Checkbox::new(("read-task", i))
                                    .checked(checked)
                                    .disabled(locked)
                                    .on_click(move |_, w, cx| {
                                        if let Some(marker) = marker {
                                            let _ = owner.update(cx, |app, cx| {
                                                app.toggle_read_task(editor_id, marker, w, cx)
                                            });
                                        }
                                    }),
                            ),
                    );
                } else {
                    row = row.child(div().w(px(16.)).h(line_height).flex().items_center().child(if tasks.ordered {
                        format!("{}.", tasks.start + i as u32)
                    } else {
                        "•".into()
                    }));
                }
                let mut body = v_flex().flex_1().min_w_0();
                for (j, (source, start)) in item.children.iter().enumerate() {
                    if let Some(start) = start.checked_sub(prefix_len) {
                        body = body.child(decorate(
                            TextView::markdown(("task-body", i * 1000 + j), source.clone())
                                .w_full()
                                .selectable(true),
                            task_owner.clone(),
                            editor_id,
                            task_base.clone(),
                            source_offset + start,
                            0,
                        ));
                    }
                }
                list = list.child(row.child(body));
            }
            list
        })
        .table_actions(move |table, _, cx| {
            let range = table
                .span
                .clone()
                .and_then(|r| shifted_range(r, source_offset, prefix_len));
            let owner = table_owner.clone();
            // Only offer the editor for tables it can write back: HTML with
            // merged cells or captions is edited in Edit mode.
            let editable = range.clone().zip(owner.upgrade()).is_some_and(|(range, app)| {
                app.read(cx)
                    .documents
                    .iter()
                    .find(|doc| doc.editor.entity_id() == editor_id && !doc.locked)
                    .and_then(|doc| {
                        let slice = doc.editor.read(cx).text().try_slice(range).ok()?;
                        EditableTable::parse(&slice.to_string())
                    })
                    .is_some()
            });
            if !editable {
                return div().into_any_element();
            }
            Button::new("edit-table")
                .ghost()
                .small()
                .icon(IconName::Pencil)
                .label("Edit Table")
                .on_click(move |_, window, cx| {
                    if let Some(range) = range.clone() {
                        let _ = owner
                            .update(cx, |app, cx| app.open_table(editor_id, range, window, cx));
                    }
                })
                .into_any_element()
        })
}
/// A ```mermaid fence, shown as its diagram.
#[derive(Clone)]
struct MermaidBlock {
    source: String,
}

/// A diagram on a soft panel the width of the text, centred; wide ones scale
/// down to fit. A diagram with a mistake shows the mistake and its source.
fn render_mermaid(source: &str, owner: WeakEntity<Enso>, cx: &mut App) -> AnyElement {
    let theme = cx.theme().clone();
    let dark = theme.mode.is_dark();
    match crate::diagram::render(source, dark) {
        Ok(drawn) => {
            let [r, g, b, _] = crate::diagram::background(dark);
            let full = crate::media::Lightbox::Diagram {
                svg: drawn.svg.clone(),
                width: drawn.width,
                height: drawn.height,
            };
            let opener = owner.clone();
            div()
                .relative()
                .group("enso-diagram")
                // Double-click to see it full size, as the toolbar's button does.
                .on_mouse_down(MouseButton::Left, move |event, w, cx| {
                    if event.click_count == 2 {
                        cx.stop_propagation();
                        let full = full.clone();
                        let _ = opener.update(cx, |app, cx| app.open_lightbox(full, w, cx));
                    }
                })
                .child(diagram_toolbar(source, &drawn, owner, &theme))
                .w_full()
                .my_1()
                .p_4()
                .rounded(px(10.))
                .bg(rgb(u32::from_be_bytes([0, r, g, b])))
                .border_1()
                .border_color(theme.border.opacity(0.6))
                .flex()
                .justify_center()
                .child(
                    img(diagram_image(&drawn.svg))
                        .w(px(drawn.width))
                        .h(px(drawn.height))
                        .max_w_full()
                        .object_fit(ObjectFit::Contain),
                )
                .into_any_element()
        }
        Err(problem) => v_flex()
            .w_full()
            .my_1()
            .p_3()
            .gap_2()
            .rounded(px(10.))
            .border_1()
            .border_color(theme.danger.opacity(0.4))
            .bg(theme.danger.opacity(0.06))
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .text_sm()
                    .text_color(theme.danger)
                    .child(Icon::new(IconName::TriangleAlert).size_4())
                    .child(if problem.line > 0 {
                        format!("Diagram error on line {}: {}", problem.line, problem.message)
                    } else {
                        format!("Diagram error: {}", problem.message)
                    }),
            )
            .child(
                div()
                    .text_xs()
                    .font_family(theme.mono_font_family.clone())
                    .text_color(theme.muted_foreground)
                    .child(source.trim_end().to_string()),
            )
            .into_any_element(),
    }
}

/// What a diagram offers on hover, top right: view it full size, copy it,
/// and save it as a PNG or an SVG.
fn diagram_toolbar(
    source: &str,
    drawn: &crate::diagram::Diagram,
    owner: WeakEntity<Enso>,
    theme: &gpui_kit::component::Theme,
) -> impl IntoElement {
    let source: SharedString = source.to_string().into();
    let full = crate::media::Lightbox::Diagram {
        svg: drawn.svg.clone(),
        width: drawn.width,
        height: drawn.height,
    };
    h_flex()
        .absolute()
        .top(px(6.))
        .right(px(6.))
        .gap(px(2.))
        .p(px(2.))
        .toolbar_surface(theme)
        .opacity(0.)
        .group_hover("enso-diagram", |bar| bar.opacity(1.))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child({
            let owner = owner.clone();
            ui::icon_button("diagram-full", IconName::Maximize, "View Full Size")
                .xsmall()
                .compact()
                .on_click(move |_, w, cx| {
                    cx.stop_propagation();
                    let full = full.clone();
                    let _ = owner.update(cx, |app, cx| app.open_lightbox(full, w, cx));
                })
        })
        .child({
            let owner = owner.clone();
            let source = source.clone();
            ui::icon_button("diagram-copy", IconName::Copy, "Copy Image")
                .xsmall()
                .compact()
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    let _ = owner.update(cx, |app, cx| app.copy_diagram(&source, cx));
                })
        })
        .child(
            ui::icon_button("diagram-save", IconName::Download, "Save")
                .xsmall()
                .compact()
                .dropdown_menu(move |menu, _, _| {
                    let save = |label: &'static str, format: crate::media::DiagramFormat| {
                        let owner = owner.clone();
                        let source = source.clone();
                        PopupMenuItem::new(label).on_click(move |_, w, cx| {
                            let _ = owner.update(cx, |app, cx| {
                                let folder = app.save_folder();
                                app.save_diagram(&source, format, folder, w, cx)
                            });
                        })
                    };
                    menu.item(save("Save as PNG…", crate::media::DiagramFormat::Png))
                        .item(save("Save as SVG…", crate::media::DiagramFormat::Svg))
                }),
        )
}

/// The SVG as an image GPUI can draw, made once per drawing: the image's id
/// is a hash of its bytes, which isn't worth recomputing every frame.
pub(crate) fn diagram_image(svg: &std::sync::Arc<str>) -> std::sync::Arc<gpui_kit::Image> {
    thread_local! {
        static IMAGES: std::cell::RefCell<HashMap<usize, (std::sync::Arc<str>, std::sync::Arc<gpui_kit::Image>)>> =
            Default::default();
    }
    let key = std::sync::Arc::as_ptr(svg) as *const u8 as usize;
    IMAGES.with(|images| {
        let mut images = images.borrow_mut();
        if let Some((_, image)) = images.get(&key) {
            return image.clone();
        }
        if images.len() > 64 {
            images.clear();
        }
        let image = std::sync::Arc::new(gpui_kit::Image::from_bytes(
            gpui_kit::ImageFormat::Svg,
            svg.as_bytes().to_vec(),
        ));
        // Holding the SVG keeps its address from being reused by another.
        images.insert(key, (svg.clone(), image.clone()));
        image
    })
}

#[derive(Clone)]
struct TaskItem {
    checked: Option<bool>,
    marker: Option<usize>,
    children: Vec<(String, usize)>,
}
#[derive(Clone)]
struct Tasks {
    items: Vec<TaskItem>,
    ordered: bool,
    start: u32,
}
fn parse_tasks(node: &Node, context: &MarkdownParseContext<'_>) -> Option<Tasks> {
    let Node::List(list) = node else {
        return None;
    };
    if !list
        .children
        .iter()
        .any(|n| matches!(n,Node::ListItem(item) if item.checked.is_some()))
    {
        return None;
    }
    let mut items = Vec::new();
    for child in &list.children {
        let Node::ListItem(item) = child else {
            continue;
        };
        let source = context.node_source(child)?;
        let position = child.position()?;
        let marker = item
            .checked
            .and_then(|_| {
                source
                    .lines()
                    .next()?
                    .find("[ ")
                    .or_else(|| source.lines().next()?.find("[x]"))
                    .or_else(|| source.lines().next()?.find("[X]"))
            })
            .map(|i| position.start.offset + context.offset() + i);
        let children = item
            .children
            .iter()
            .filter_map(|n| {
                Some((
                    context.node_source(n)?.to_owned(),
                    n.position()?.start.offset + context.offset(),
                ))
            })
            .collect();
        items.push(TaskItem {
            checked: item.checked,
            marker,
            children,
        });
    }
    Some(Tasks {
        items,
        ordered: list.ordered,
        start: list.start.unwrap_or(1),
    })
}
impl Enso {
    pub(crate) fn toggle_read_task(
        &mut self,
        id: EntityId,
        marker: usize,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.rich_target(id, marker..marker + 3, w, cx) else {
            return;
        };
        if !matches!(target.expected.as_str(), "[ ]" | "[x]" | "[X]") {
            return;
        }
        let replacement = if target.expected == "[ ]" { "x" } else { " " };
        target.editor.update(cx, |e, cx| {
            e.set_selected_range(marker + 1..marker + 2, cx);
            e.replace(replacement, w, cx);
        });
        cx.notify();
    }
}

struct InlineImages {
    owner: WeakEntity<Enso>,
    editor_id: EntityId,
    base: Option<PathBuf>,
    offset: usize,
    prefix: usize,
}
impl MarkdownPlugin for InlineImages {
    fn name(&self) -> &str {
        "enso-inline-image"
    }
    fn parse(&self, node: &Node, context: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        if !matches!(node, Node::Image(_) | Node::Html(_)) {
            return None;
        }
        let source = context.node_source(node)?;
        let picture = picture_node(node, source)?;
        Some(
            MarkdownNode::new(self.name(), picture.clone())
                .text(picture.alt)
                .markdown(source.to_owned()),
        )
    }
    fn render(&self, node: &MarkdownNode, window: &mut Window, cx: &mut App) -> impl IntoElement {
        render_picture(
            node,
            self.owner.clone(),
            self.editor_id,
            self.base.clone(),
            self.offset,
            self.prefix,
            window,
            cx,
        )
    }
}

#[derive(Default)]
struct ResizeState {
    preview: Option<(EntityId, Range<usize>, f32)>,
}
impl Global for ResizeState {}
pub struct ImageResize {
    target: Target,
    picture: Picture,
    bounds: Bounds<Pixels>,
    available: Pixels,
    left: bool,
    percent: f32,
}
fn geometry(slot: std::sync::Arc<std::sync::Mutex<Bounds<Pixels>>>) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, _, _| {
            *slot.lock().unwrap() = bounds;
        },
    )
    .absolute()
    .inset_0()
}
fn render_picture(
    node: &MarkdownNode,
    owner: WeakEntity<Enso>,
    editor_id: EntityId,
    base: Option<PathBuf>,
    offset: usize,
    prefix: usize,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let picture = node.data::<Picture>().unwrap().clone();
    // A locked note's images can be viewed, copied and saved, not changed.
    let locked = owner.upgrade().is_some_and(|app| app.read(cx).is_locked(editor_id));
    let source = if !picture.src.contains(':') {
        ImageSource::from(
            base.as_ref()
                .map(|b| b.join(&picture.src))
                .unwrap_or_else(|| PathBuf::from(&picture.src)),
        )
    } else {
        ImageSource::from(SharedString::from(picture.src.clone()))
    };
    let intrinsic = match &source {
        ImageSource::Resource(resource) => window
            .use_asset::<ImageAssetLoader>(resource, cx)
            .and_then(Result::ok)
            .map(|image| image.size(0)),
        ImageSource::Render(image) => Some(image.size(0)),
        _ => None,
    };
    let ratio = intrinsic
        .filter(|s| s.height.0 > 0)
        .map(|s| s.width.0 as f32 / s.height.0 as f32)
        .unwrap_or(1.);
    let range = node
        .source_range()
        .and_then(|r| shifted_range(r, offset, prefix));
    let expected = node.as_markdown().to_owned();
    let preview = cx
        .try_global::<ResizeState>()
        .and_then(|s| s.preview.as_ref())
        .filter(|(id, r, _)| *id == editor_id && Some(r) == range.as_ref())
        .map(|(_, _, pct)| *pct);
    let outer = Arc::new(std::sync::Mutex::new(Bounds::default()));
    let inner = Arc::new(std::sync::Mutex::new(Bounds::default()));
    let mut image = div()
        .relative()
        .group("enso-image")
        .w(preview
            .map(|p| relative(p / 100.).into())
            .unwrap_or_else(|| dimension(&picture.width).unwrap_or(relative(1.).into())))
        .debug_selector(|| "image-frame".into())
        .max_w_full()
        .aspect_ratio(ratio)
        .map(|container| {
            if preview.is_none() {
                if let Some(height) = dimension(&picture.height) {
                    return container.h(height);
                }
            }
            container
        })
        // Percent widths on GPUI's Img otherwise retain the original pixel
        // height. The aspect-ratio container owns both dimensions; the image
        // fills it without contributing an intrinsic-height layout box.
        .child(img(source).absolute().inset_0().size_full())
        .child(geometry(inner.clone()));
    for left in [true, false].into_iter().filter(|_| !locked) {
        let owner = owner.clone();
        let range = range.clone();
        let expected = expected.clone();
        let picture = picture.clone();
        let outer = outer.clone();
        let inner = inner.clone();
        image = image.child(
            div()
                .id(if left {
                    "resize-image-left"
                } else {
                    "resize-image-right"
                })
                .debug_selector(move || {
                    if left {
                        "image-handle-left".into()
                    } else {
                        "image-handle-right".into()
                    }
                })
                .absolute()
                .top(relative(0.5))
                .mt(px(-24.))
                .w(px(14.))
                .h(px(48.))
                .map(|d| {
                    if left {
                        d.left(px(0.))
                    } else {
                        d.right(px(0.))
                    }
                })
                .flex()
                .items_center()
                .justify_center()
                .cursor(CursorStyle::ResizeLeftRight)
                .opacity(if preview.is_some() { 1. } else { 0. })
                .group_hover("enso-image", |d| d.opacity(1.))
                .child(
                    div()
                        .w(px(4.))
                        .h(px(32.))
                        .rounded_full()
                        .bg(black().opacity(0.45))
                        .border_1()
                        .border_color(white().opacity(0.85)),
                )
                .on_click(|_, _, cx| cx.stop_propagation())
                .on_mouse_down(MouseButton::Left, move |_, w, cx| {
                    cx.stop_propagation();
                    if let Some(range) = range.clone() {
                        let bounds = *inner.lock().unwrap();
                        let available = outer.lock().unwrap().size.width;
                        let _ = owner.update(cx, |app, cx| {
                            app.start_image_resize(
                                editor_id,
                                range,
                                expected.clone(),
                                picture.clone(),
                                bounds,
                                available,
                                left,
                                w,
                                cx,
                            )
                        });
                    }
                }),
        );
    }
    let theme = cx.theme().clone();
    let mut toolbar = h_flex()
        .absolute()
        .top(px(6.))
        .right(px(6.))
        .gap(px(2.))
        .p(px(2.))
        .toolbar_surface(&theme)
        .opacity(0.)
        .group_hover("enso-image", |bar| bar.opacity(1.))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
    for (alignment, icon, label) in (!locked).then_some([
        (
            Alignment::Left,
            IconName::AlignHorizontalJustifyStart,
            "Align left",
        ),
        (
            Alignment::Center,
            IconName::AlignHorizontalJustifyCenter,
            "Align center",
        ),
        (
            Alignment::Right,
            IconName::AlignHorizontalJustifyEnd,
            "Align right",
        ),
    ]).into_iter().flatten() {
        let owner = owner.clone();
        let range = range.clone();
        let expected = expected.clone();
        let picture = picture.clone();
        toolbar = toolbar.child(
            div()
                .debug_selector(move || format!("image-align-{alignment:?}").to_lowercase())
                .child(
                    ui::icon_button(label, icon, label)
                        .xsmall()
                        .compact()
                        .when(picture.alignment == alignment, |button| {
                            button.bg(theme.accent)
                        })
                        .on_click(move |_, w, cx| {
                            cx.stop_propagation();
                            if let Some(range) = range.clone() {
                                let _ = owner.update(cx, |app, cx| {
                                    app.align_picture(
                                        editor_id,
                                        range,
                                        expected.clone(),
                                        picture.clone(),
                                        alignment,
                                        w,
                                        cx,
                                    )
                                });
                            }
                        }),
                ),
        );
    }
    // The file behind the picture, for viewing full size, copying and saving
    // it. Web images only exist in GPUI's cache, so they have neither.
    let local = crate::export::html::local_path(&picture.src, base.as_deref()).filter(|p| p.is_file());
    let full = match &local {
        Some(path) => Some(crate::media::Lightbox::Image(path.clone())),
        None if picture.src.starts_with("https://") || picture.src.starts_with("http://") => {
            Some(crate::media::Lightbox::Web(picture.src.clone().into()))
        }
        None => None,
    };
    if let Some(full) = full.clone() {
        let owner = owner.clone();
        toolbar = toolbar
            .when(!locked, |t| t.child(div().w(px(1.)).h(px(14.)).mx(px(2.)).bg(theme.border)))
            .child(
                ui::icon_button("image-full", IconName::Maximize, "View Full Size")
                    .xsmall()
                    .compact()
                    .on_click(move |_, w, cx| {
                        cx.stop_propagation();
                        let full = full.clone();
                        let _ = owner.update(cx, |app, cx| app.open_lightbox(full, w, cx));
                    }),
            );
    }
    // A web image copies and saves by downloading it.
    if local.is_none() && (picture.src.starts_with("https://") || picture.src.starts_with("http://")) {
        let url: SharedString = picture.src.clone().into();
        toolbar = toolbar
            .child({
                let owner = owner.clone();
                let url = url.clone();
                ui::icon_button("image-copy", IconName::Copy, "Copy Image")
                    .xsmall()
                    .compact()
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        let _ = owner.update(cx, |app, cx| app.copy_web_image(url.clone(), cx));
                    })
            })
            .child({
                let owner = owner.clone();
                ui::icon_button("image-save", IconName::Download, "Save Image As…")
                    .xsmall()
                    .compact()
                    .on_click(move |_, w, cx| {
                        cx.stop_propagation();
                        let _ = owner.update(cx, |app, cx| app.save_web_image(url.clone(), w, cx));
                    })
            })
            .when(!locked, |t| t.child(div().w(px(1.)).h(px(14.)).mx(px(2.)).bg(theme.border)));
    }
    if let Some(path) = local {
        toolbar = toolbar
            .child({
                let owner = owner.clone();
                let path = path.clone();
                ui::icon_button("image-copy", IconName::Copy, "Copy Image")
                    .xsmall()
                    .compact()
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        let _ = owner.update(cx, |app, cx| app.copy_image(path.clone(), cx));
                    })
            })
            .child({
                let owner = owner.clone();
                ui::icon_button("image-save", IconName::Download, "Save Image As…")
                    .xsmall()
                    .compact()
                    .on_click(move |_, w, cx| {
                        cx.stop_propagation();
                        let _ = owner.update(cx, |app, cx| app.save_image_as(path.clone(), w, cx));
                    })
            })
            .when(!locked, |t| t.child(div().w(px(1.)).h(px(14.)).mx(px(2.)).bg(theme.border)));
    }
    let opener = owner.clone();
    toolbar = toolbar.when(!locked, |toolbar| toolbar.child(
        div().debug_selector(|| "image-edit-button".into()).child(
            ui::icon_button("edit-image", IconName::Pencil, "Edit image")
                .xsmall()
                .compact()
                .on_click(move |_, w, cx| {
                    cx.stop_propagation();
                    if let Some(range) = range.clone() {
                        let _ = owner.update(cx, |app, cx| {
                            app.open_picture(
                                editor_id,
                                range,
                                expected.clone(),
                                picture.clone(),
                                w,
                                cx,
                            )
                        });
                    }
                }),
        ),
    ));
    let alignment = node.data::<Picture>().unwrap().alignment;
    div()
        .id(("editable-image", node.source_range().map_or(0, |r| r.start)))
        // Double-click to see it full size.
        .on_mouse_down(MouseButton::Left, move |event, w, cx| {
            if event.click_count == 2 {
                if let Some(full) = full.clone() {
                    cx.stop_propagation();
                    let _ = opener.update(cx, |app, cx| app.open_lightbox(full, w, cx));
                }
            }
        })
        .debug_selector(|| "image-available".into())
        .relative()
        .w_full()
        .flex()
        .map(|container| match alignment {
            Alignment::Left => container.justify_start(),
            Alignment::Center => container.justify_center(),
            Alignment::Right => container.justify_end(),
        })
        .child(geometry(outer))
        .child(image.child(toolbar))
        .into_any_element()
}
impl Enso {
    fn align_picture(
        &mut self,
        id: EntityId,
        range: Range<usize>,
        expected: String,
        mut picture: Picture,
        alignment: Alignment,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if picture.alignment == alignment || self.is_locked(id) {
            return;
        }
        let Some(target) = self.rich_target(id, range, w, cx) else {
            return;
        };
        if target.expected != expected {
            self.message("The image changed. Try its controls again.", true, cx);
            return;
        }
        picture.alignment = alignment;
        target.editor.update(cx, |e, cx| {
            e.set_selected_range(target.range, cx);
            e.replace(picture.html(), w, cx);
        });
        cx.notify();
    }
    fn start_image_resize(
        &mut self,
        id: EntityId,
        range: Range<usize>,
        expected: String,
        picture: Picture,
        bounds: Bounds<Pixels>,
        available: Pixels,
        left: bool,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if available <= px(0.) || self.is_locked(id) {
            return;
        }
        let Some(target) = self.rich_target(id, range, w, cx) else {
            return;
        };
        if target.expected != expected {
            return;
        }
        let percent = (f32::from(bounds.size.width) / f32::from(available) * 100.).clamp(5., 100.);
        cx.set_global(ResizeState {
            preview: Some((id, target.range.clone(), percent)),
        });
        self.image_resize = Some(ImageResize {
            target,
            picture,
            bounds,
            available,
            left,
            percent,
        });
        cx.notify();
    }
    pub fn move_image_resize(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(session) = &mut self.image_resize else {
            return;
        };
        let width = if session.left {
            session.bounds.right() - event.position.x
        } else {
            event.position.x - session.bounds.left()
        };
        session.percent = (f32::from(width) / f32::from(session.available) * 100.).clamp(5., 100.);
        cx.set_global(ResizeState {
            preview: Some((
                session.target.editor.entity_id(),
                session.target.range.clone(),
                session.percent,
            )),
        });
        if let Some(doc) = self
            .documents
            .iter()
            .find(|d| d.editor.entity_id() == session.target.editor.entity_id())
        {
            if let Some(reader) = &doc.large_reader {
                reader.update(cx, |r, cx| r.refresh_layout(cx));
            } else {
                doc.preview
                    .update(cx, |v, cx| v.invalidate_inline_layout(cx));
            }
        }
        cx.notify();
    }
    pub fn finish_image_resize(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let Some(mut session) = self.image_resize.take() else {
            return;
        };
        cx.set_global(ResizeState { preview: None });
        session.picture.width = format!("{:.1}%", session.percent);
        session.picture.height.clear();
        if session
            .target
            .editor
            .read(cx)
            .value()
            .get(session.target.range.clone())
            == Some(session.target.expected.as_str())
        {
            session.target.editor.update(cx, |e, cx| {
                e.set_selected_range(session.target.range, cx);
                e.replace(session.picture.html(), w, cx);
            });
        }
        cx.notify();
    }
}

fn shifted_range(range: Range<usize>, offset: usize, prefix: usize) -> Option<Range<usize>> {
    if range.start < prefix {
        return None;
    }
    Some(range.start - prefix + offset..range.end - prefix + offset)
}
struct Target {
    editor: Entity<EditorState>,
    range: Range<usize>,
    expected: String,
    focus: Option<FocusHandle>,
}
pub struct ImageForm {
    target: Target,
    picture: Picture,
    alt: Entity<InputState>,
}
pub struct TableForm {
    target: Target,
    cells: Vec<Vec<Entity<InputState>>>,
    aligns: Vec<String>,
    syntax: TableSyntax,
}

/// How a table is written in the source, so saving writes it back the same way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TableSyntax {
    /// A pipe table; `aligns` holds its delimiter row.
    Markdown,
    /// A plain `<table>`; `header` says whether its first row is `<th>` cells.
    Html { header: bool },
}

/// A table the table editor can open: its rows (header first) and how to
/// write them back.
#[derive(Debug, PartialEq)]
pub(crate) struct EditableTable {
    pub(crate) rows: Vec<Vec<String>>,
    aligns: Vec<String>,
    pub(crate) syntax: TableSyntax,
}

impl EditableTable {
    fn parse(source: &str) -> Option<Self> {
        if source.trim_start().starts_with('<') {
            return Self::parse_html(source);
        }
        let mut rows: Vec<_> = source.lines().map(source_cells).collect();
        if rows.len() < 2 || rows[0].is_empty() {
            return None;
        }
        let cols = rows[0].len();
        let mut aligns = rows.remove(1);
        aligns.resize(cols, "---".into());
        for row in &mut rows {
            row.resize(cols, String::new());
        }
        Some(Self { rows, aligns, syntax: TableSyntax::Markdown })
    }

    /// Plain HTML tables only: rows of `<th>`/`<td>` cells, optionally in
    /// `<thead>`/`<tbody>`. Merged cells, captions, column groups and nested
    /// tables can't be shown as a grid, so those tables aren't editable.
    pub(crate) fn parse_html(source: &str) -> Option<Self> {
        let text = source.trim();
        // ASCII lowercasing keeps byte offsets, so `lower` indexes `text`.
        let lower = text.to_ascii_lowercase();
        if !lower.starts_with("<table") || !lower.ends_with("</table>") {
            return None;
        }
        let inner_start = lower.find('>')? + 1;
        let inner_end = lower.len() - "</table>".len();
        let inner = &lower[inner_start..inner_end];
        if ["<table", "<caption", "<colgroup", "<col ", "<col>", "colspan", "rowspan"]
            .iter()
            .any(|tag| inner.contains(tag))
        {
            return None;
        }
        // The start of the next `<name` tag (not `<names…`) at or after `from`.
        let open = |name: &str, from: usize| -> Option<usize> {
            let mut at = from;
            loop {
                let i = lower[at..inner_end].find(&format!("<{name}"))? + at;
                let next = lower.as_bytes().get(i + name.len() + 1)?;
                if *next == b'>' || next.is_ascii_whitespace() {
                    return Some(i);
                }
                at = i + 1;
            }
        };
        let mut rows = Vec::new();
        let mut header = false;
        let mut at = inner_start;
        while let Some(row_start) = open("tr", at) {
            let row_end = lower[row_start..inner_end].find("</tr>")? + row_start;
            let mut cells = Vec::new();
            let mut cell_at = row_start + 3;
            loop {
                let next = [("th", open("th", cell_at)), ("td", open("td", cell_at))]
                    .into_iter()
                    .filter_map(|(name, i)| Some((name, i.filter(|&i| i < row_end)?)))
                    .min_by_key(|(_, i)| *i);
                let Some((name, start)) = next else { break };
                let content_start = lower[start..row_end].find('>')? + start + 1;
                let close = lower[content_start..row_end].find(&format!("</{name}>"))?
                    + content_start;
                if rows.is_empty() && cells.is_empty() {
                    header = name == "th";
                }
                cells.push(
                    text[content_start..close]
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" "),
                );
                cell_at = close + name.len() + 3;
            }
            if cells.is_empty() {
                return None;
            }
            rows.push(cells);
            at = row_end + "</tr>".len();
        }
        let cols = rows.iter().map(Vec::len).max()?;
        for row in &mut rows {
            row.resize(cols, String::new());
        }
        Some(Self { rows, aligns: vec!["---".into(); cols], syntax: TableSyntax::Html { header } })
    }

    /// The table written back in its own syntax.
    fn source(rows: &[Vec<String>], aligns: &[String], syntax: TableSyntax) -> String {
        match syntax {
            TableSyntax::Markdown => table_source(rows, aligns),
            TableSyntax::Html { header } => {
                let line = |tag: &str, row: &Vec<String>| {
                    let cells: String = row
                        .iter()
                        .map(|c| format!("<{tag}>{}</{tag}>", c.replace(['\r', '\n'], " ")))
                        .collect();
                    format!("    <tr>{cells}</tr>")
                };
                let mut lines = vec!["<table>".to_string()];
                let body = if header && !rows.is_empty() {
                    lines.push("  <thead>".into());
                    lines.push(line("th", &rows[0]));
                    lines.push("  </thead>".into());
                    &rows[1..]
                } else {
                    rows
                };
                if !body.is_empty() {
                    lines.push("  <tbody>".into());
                    lines.extend(body.iter().map(|row| line("td", row)));
                    lines.push("  </tbody>".into());
                }
                lines.push("</table>".into());
                lines.join("\n")
            }
        }
    }
}
pub struct HighlightForm {
    target: Target,
    focus: FocusHandle,
    body: String,
    picker: Entity<ColorPickerState>,
    _subscription: Subscription,
}
fn highlight_hex(color: Hsla) -> String {
    let color = Rgba::from(color);
    let channel = |v: f32| (v.clamp(0., 1.) * 255.).round() as u8;
    if color.a < 1. {
        format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            channel(color.r),
            channel(color.g),
            channel(color.b),
            channel(color.a)
        )
    } else {
        format!(
            "#{:02x}{:02x}{:02x}",
            channel(color.r),
            channel(color.g),
            channel(color.b)
        )
    }
}
fn existing_highlight(
    source: &str,
    selection: &Range<usize>,
) -> Option<(Range<usize>, String, Hsla)> {
    let mut offset = 0;
    while let Some(start) = source[offset..].find("<mark").map(|i| i + offset) {
        let end_tag = source[start..].find('>')? + start + 1;
        if !source
            .as_bytes()
            .get(start + 5)
            .is_some_and(|b| b.is_ascii_whitespace() || *b == b'>')
        {
            offset = end_tag;
            continue;
        }
        let close = source[end_tag..].find("</mark>")? + end_tag;
        let end = close + 7;
        if selection.start >= start && selection.end <= end {
            let attrs = attributes(&source[start..end_tag]);
            let raw = attrs.get("color").cloned().or_else(|| {
                attrs.get("style").and_then(|style| {
                    style.split(';').find_map(|v| {
                        let (name, value) = v.split_once(':')?;
                        (name.trim().eq_ignore_ascii_case("background-color"))
                            .then(|| value.trim().to_owned())
                    })
                })
            });
            let color = raw
                .and_then(|raw| match raw.to_ascii_lowercase().as_str() {
                    "blue" => Some(rgb(0x3b82f6).into()),
                    "yellow" => Some(rgb(0xfacc15).into()),
                    "black" => Some(rgb(0x000000).into()),
                    "white" => Some(rgb(0xffffff).into()),
                    _ => Rgba::try_from(raw.as_str()).ok().map(Hsla::from),
                })
                .unwrap_or_else(|| rgb(0xfef08a).into());
            return Some((start..end, source[end_tag..close].to_owned(), color));
        }
        offset = end;
    }
    None
}

pub enum Controls {
    Highlight(HighlightForm),
    Image(ImageForm),
    Table(TableForm),
}
fn input(value: &str, window: &mut Window, cx: &mut App) -> Entity<InputState> {
    cx.new(|cx| InputState::new(window, cx).default_value(value))
}
fn source_cells(line: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut escaped = false;
    let line = line.trim().trim_start_matches('|');
    for c in line.chars() {
        if escaped {
            cell.push('\\');
            cell.push(c);
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '|' {
            cells.push(cell.trim().to_owned());
            cell.clear();
        } else {
            cell.push(c);
        }
    }
    if escaped {
        cell.push('\\');
    }
    if !cell.trim().is_empty() || !line.ends_with('|') {
        cells.push(cell.trim().to_owned());
    }
    cells
}
fn table_source(rows: &[Vec<String>], aligns: &[String]) -> String {
    let mut lines = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        lines.push(format!(
            "| {} |",
            row.iter()
                .map(|c| {
                    // Already escaped pipes stay escaped; newly typed pipes are escaped.
                    let mut out = String::new();
                    let mut escaped = false;
                    for ch in c.replace(['\r', '\n'], " ").chars() {
                        if ch == '|' && !escaped {
                            out.push('\\');
                        }
                        out.push(ch);
                        escaped = ch == '\\' && !escaped;
                    }
                    out
                })
                .collect::<Vec<_>>()
                .join(" | ")
        ));
        if i == 0 {
            lines.push(format!("| {} |", aligns.join(" | ")));
        }
    }
    lines.join("\n")
}
impl Enso {
    pub(super) fn open_highlight(
        &mut self,
        editor: Entity<EditorState>,
        slash: bool,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !editor.read(cx).is_editable() {
            return;
        }
        let selection = editor.read(cx).selected_range();
        let source = editor.read(cx).value();
        let (range, body, color) = if !slash {
            existing_highlight(&source, &selection).unwrap_or_else(|| {
                let text = &source[selection.clone()];
                (
                    selection.clone(),
                    escape(if text.is_empty() { "highlight" } else { text }),
                    rgb(0xfef08a).into(),
                )
            })
        } else {
            (selection, "highlight".into(), rgb(0xfef08a).into())
        };
        let Some(target) = self.rich_target(editor.entity_id(), range, w, cx) else {
            return;
        };
        let picker = cx.new(|cx| ColorPickerState::new(w, cx).default_value(color));
        let subscription = cx.observe(&picker, |_, _, cx| cx.notify());
        let focus = cx.focus_handle();
        editor.update(cx, |e, cx| e.set_selection_focus(Some(focus.clone()), cx));
        focus.focus(w, cx);
        self.closing_controls = None;
        self.controls = Some(Controls::Highlight(HighlightForm {
            target,
            body,
            picker,
            focus,
            _subscription: subscription,
        }));
        cx.notify();
    }
    fn choose_highlight_color(&mut self, color: Hsla, w: &mut Window, cx: &mut Context<Self>) {
        if let Some(Controls::Highlight(form)) = &self.controls {
            form.picker
                .update(cx, |picker, cx| picker.set_value(color, w, cx));
        }
        cx.notify();
    }
    fn rich_target(
        &mut self,
        id: EntityId,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Target> {
        // A locked note is read, not changed: no control may edit it.
        if self.is_locked(id) {
            self.message("This page is locked. Unlock it to make changes.", true, cx);
            return None;
        }
        let doc = self
            .documents
            .iter_mut()
            .find(|d| d.editor.entity_id() == id)?;
        if !doc.editor_loaded {
            doc.editor
                .update(cx, |e, cx| e.set_value(doc.saved.as_str(), window, cx));
            doc.editor_loaded = true;
        }
        let value = doc.editor.read(cx).value();
        let expected = value.get(range.clone())?.to_owned();
        Some(Target {
            editor: doc.editor.clone(),
            range,
            expected,
            focus: window.focused(cx),
        })
    }
    fn open_picture(
        &mut self,
        id: EntityId,
        range: Range<usize>,
        expected: String,
        picture: Picture,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.rich_target(id, range, window, cx) else {
            return;
        };
        if target.expected != expected {
            self.message("The image changed. Click it again to edit.", true, cx);
            return;
        }
        let alt = input(&picture.alt, window, cx);
        alt.focus_handle(cx).focus(window, cx);
        self.closing_controls = None;
        self.controls = Some(Controls::Image(ImageForm {
            target,
            picture,
            alt,
        }));
        cx.notify();
    }
    fn open_table(
        &mut self,
        id: EntityId,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.rich_target(id, range, window, cx) else {
            return;
        };
        let Some(table) = EditableTable::parse(&target.expected) else {
            return;
        };
        let cells: Vec<Vec<_>> = table
            .rows
            .iter()
            .map(|row| row.iter().map(|v| input(v, window, cx)).collect())
            .collect();
        cells[0][0].focus_handle(cx).focus(window, cx);
        self.closing_controls = None;
        self.controls = Some(Controls::Table(TableForm {
            target,
            cells,
            aligns: table.aligns,
            syntax: table.syntax,
        }));
        cx.notify();
    }
    pub fn close_controls(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(form) = self.controls.take() {
            let target = match &form {
                Controls::Highlight(f) => &f.target,
                Controls::Image(f) => &f.target,
                Controls::Table(f) => &f.target,
            };
            if matches!(&form, Controls::Highlight(_)) {
                target
                    .editor
                    .update(cx, |e, cx| e.set_selection_focus(None, cx));
            }
            if let Some(focus) = &target.focus {
                focus.focus(window, cx);
            }
            // Stop actions immediately, but retain the surface for its exit.
            self.closing_controls = Some(form);
            cx.notify();
        }
    }
    pub(super) fn save_controls(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.controls.as_mut() else {
            return;
        };
        let (target, replacement) = match form {
            Controls::Highlight(form) => {
                let color = form
                    .picker
                    .read(cx)
                    .value()
                    .unwrap_or_else(|| rgb(0xfef08a).into());
                (
                    &form.target,
                    format!(
                        "<mark color=\"{}\">{}</mark>",
                        highlight_hex(color),
                        form.body
                    ),
                )
            }
            Controls::Image(form) => {
                form.picture.alt = form.alt.read(cx).value().to_string();
                (&form.target, form.picture.html())
            }
            Controls::Table(form) => {
                let rows: Vec<_> = form
                    .cells
                    .iter()
                    .map(|row| row.iter().map(|c| c.read(cx).value().to_string()).collect())
                    .collect();
                (&form.target, EditableTable::source(&rows, &form.aligns, form.syntax))
            }
        };
        if !self
            .documents
            .iter()
            .any(|d| d.editor.entity_id() == target.editor.entity_id())
            || target.editor.read(cx).value().get(target.range.clone())
                != Some(target.expected.as_str())
        {
            self.message("This content changed while its controls were open. Reopen the controls to edit it.",true,cx);
            return;
        }
        target.editor.update(cx, |e, cx| {
            e.set_selected_range(target.range.clone(), cx);
            e.replace(replacement, window, cx);
        });
        self.close_controls(window, cx);
    }
    fn replace_picture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Controls::Image(form)) = &self.controls else {
            return;
        };
        let id = form.target.editor.entity_id();
        let range = form.target.range.clone();
        let Some(doc) = self.documents.iter().find(|d| d.editor.entity_id() == id) else {
            return;
        };
        let Some(root) = self.root.clone().or_else(|| {
            doc.path
                .as_ref()
                .and_then(|p| p.parent())
                .map(PathBuf::from)
        }) else {
            self.message("Save this note before replacing its image.", true, cx);
            return;
        };
        let base = doc
            .path
            .as_ref()
            .and_then(|p| p.parent())
            .map(PathBuf::from)
            .unwrap_or_else(|| root.clone());
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Replace image".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next().filter(|p| files::is_image(p)) else {
                return;
            };
            let result = cx
                .background_executor()
                .spawn(async move { files::import_image(&root, &path) })
                .await;
            let _ = this.update_in(cx, |app, _, cx| match result {
                Ok(path) => {
                    if let Some(Controls::Image(form)) = &mut app.controls {
                        if form.target.editor.entity_id() == id && form.target.range == range {
                            form.picture.src = files::relative_link(&base, &path);
                            cx.notify();
                        }
                    }
                }
                Err(e) => app.message(e.to_string(), true, cx),
            });
        })
        .detach();
    }
    fn table_structure(
        &mut self,
        row: usize,
        col: usize,
        action: u8,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(Controls::Table(form)) = &mut self.controls else {
            return;
        };
        match action {
            0 => {
                let cells = (0..form.aligns.len())
                    .map(|_| input("", window, cx))
                    .collect();
                form.cells.insert((row + 1).min(form.cells.len()), cells);
            }
            1 if row > 0 && form.cells.len() > 1 => {
                form.cells.remove(row);
            }
            2 => {
                for row in &mut form.cells {
                    row.insert(col + 1, input("", window, cx));
                }
                form.aligns.insert(col + 1, "---".into());
            }
            3 if form.aligns.len() > 1 => {
                for row in &mut form.cells {
                    row.remove(col);
                }
                form.aligns.remove(col);
            }
            _ => {}
        }
        cx.notify();
    }
    pub fn controls_overlay(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let motion = cx.theme().motion_tokens();
        // Sample the closed state too, so the first opening starts at zero.
        let progress = transition(
            "object-modal-presence",
            if self.controls.is_some() {
                1.0_f32
            } else {
                0.0
            },
            Transition::new(std::time::Duration::from_millis(180))
                .easing(motion.easing_move.clone()),
            window,
            cx,
        )
        .clamp(0., 1.);
        if self.controls.is_none() && progress <= 0.001 {
            self.closing_controls = None;
            return None;
        }
        let form = self.controls.as_ref().or(self.closing_controls.as_ref())?;
        let modal_focus = match form {
            Controls::Highlight(form) => Some(form.focus.clone()),
            _ => None,
        };
        let theme = cx.theme().clone();
        let mut panel = v_flex().gap_3().p_4().w_full();
        match form {
            Controls::Highlight(form) => {
                let color = form
                    .picker
                    .read(cx)
                    .value()
                    .unwrap_or_else(|| rgb(0xfef08a).into());
                let presets = [
                    ("Lemon", 0xfef08a),
                    ("Apricot", 0xfed7aa),
                    ("Rose", 0xfecaca),
                    ("Pink", 0xfbcfe8),
                    ("Lavender", 0xddd6fe),
                    ("Sky", 0xbfdbfe),
                    ("Mint", 0xbbf7d0),
                ];
                let name = presets
                    .iter()
                    .find(|(_, value)| highlight_hex(rgb(*value).into()) == highlight_hex(color))
                    .map_or("Custom", |(name, _)| *name);
                panel = panel
                    .gap_4()
                    .child(
                        h_flex()
                            .justify_between()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(
                                        Icon::new(IconName::Highlighter)
                                            .size(px(16.))
                                            .text_color(theme.muted_foreground),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child("Highlight"),
                                    ),
                            )
                            .child(
                                h_flex()
                                    .gap_1p5()
                                    .child(div().size(px(6.)).rounded_full().bg(color))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .child(name),
                                    ),
                            ),
                    )
                    .child(h_flex().gap(px(6.)).justify_between().children(
                        presets.into_iter().enumerate().map(|(i, (name, value))| {
                            let swatch: Hsla = rgb(value).into();
                            let selected = highlight_hex(swatch) == highlight_hex(color);
                            // A plain tile rather than a custom Button: the kit's
                            // custom variant mixes its color 80% toward
                            // transparent, which washes the swatch out.
                            let hover = swatch.blend(black().opacity(0.06));
                            let active = swatch.blend(black().opacity(0.12));
                            div()
                                .debug_selector(move || format!("highlight-swatch-{i}"))
                                .child(
                                    div()
                                        .id(("highlight-color", i))
                                        .size(px(36.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(10.))
                                        .bg(swatch)
                                        .border_1()
                                        .border_color(if selected {
                                            theme.foreground.opacity(0.45)
                                        } else {
                                            swatch.blend(black().opacity(0.08))
                                        })
                                        .text_color(rgb(0x303235))
                                        .cursor_pointer()
                                        .hover(move |tile| tile.bg(hover))
                                        .active(move |tile| tile.bg(active))
                                        .tooltip(move |window, cx| {
                                            Tooltip::new(name).build(window, cx)
                                        })
                                        .child(
                                            Icon::new(IconName::Check)
                                                .size(px(14.))
                                                .opacity(if selected { 1. } else { 0. }),
                                        )
                                        .on_click(cx.listener(move |app, _, w, cx| {
                                            app.choose_highlight_color(swatch, w, cx)
                                        })),
                                )
                        }),
                    ));
            }
            Controls::Image(form) => {
                panel = panel
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("Image"))
                    .child(div().text_sm().child("Alt text"))
                    .child(Input::new(&form.alt))
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .truncate()
                            .child(form.picture.src.clone()),
                    )
                    .child(
                        ui::secondary_button("replace-image", "Replace Image…")
                            .on_click(cx.listener(|app, _, w, cx| app.replace_picture(w, cx))),
                    );
            }
            Controls::Table(form) => {
                panel = panel.child(div().font_weight(FontWeight::SEMIBOLD).child("Edit Table"));
                let weak = cx.entity().downgrade();
                let mut grid = v_flex()
                    .id("table-cells")
                    .gap_1()
                    .overflow_scroll()
                    .max_h(px(400.));
                for (r, row) in form.cells.iter().enumerate() {
                    let mut line = h_flex().gap_1();
                    for (c, cell) in row.iter().enumerate() {
                        let weak = weak.clone();
                        let columns = form.aligns.len();
                        let group: SharedString = format!("table-cell-{r}-{c}").into();
                        line = line.child(
                            div()
                                .id(("table-cell", r * columns + c))
                                .group(group.clone())
                                .relative()
                                .min_w(px(110.))
                                .flex_1()
                                .child(Input::new(cell).pr(px(28.)))
                                .child(
                                    div()
                                        .absolute()
                                        .right(px(3.))
                                        .top(relative(0.5))
                                        .mt(px(-10.))
                                        .opacity(0.)
                                        .group_hover(group, |menu| menu.opacity(1.))
                                        .debug_selector(move || format!("table-menu-{r}-{c}"))
                                        .child(
                                            ui::icon_button(
                                                ("table-menu", r * columns + c),
                                                IconName::Ellipsis,
                                                "Row and column options",
                                            )
                                            .xsmall()
                                            .compact()
                                            .tab_stop(false)
                                                .dropdown_menu(move |menu, _, _| {
                                                    let mut menu = menu;
                                                    for (action, label) in [
                                                        (0, "Add Row Below"),
                                                        (1, "Remove Row"),
                                                        (2, "Add Column After"),
                                                        (3, "Remove Column"),
                                                    ] {
                                                        let weak = weak.clone();
                                                        menu = menu.item(
                                                            PopupMenuItem::new(label)
                                                                .disabled(
                                                                    (action == 1 && r == 0)
                                                                        || (action == 3
                                                                            && columns == 1),
                                                                )
                                                                .on_click(move |_, w, cx| {
                                                                    let _ = weak.update(
                                                                        cx,
                                                                        |app, cx| {
                                                                            app.table_structure(
                                                                                r, c, action, w, cx,
                                                                            )
                                                                        },
                                                                    );
                                                                }),
                                                        );
                                                    }
                                                    menu
                                                }),
                                        ),
                                ),
                        );
                    }
                    grid = grid.child(line);
                }
                panel =
                    panel.child(grid).child(
                        h_flex()
                            .gap_2()
                            .child(ui::secondary_button("add-row", "+ Row").on_click(
                                cx.listener(|app, _, w, cx| {
                                    let row = match &app.controls {
                                        Some(Controls::Table(f)) => f.cells.len() - 1,
                                        _ => return,
                                    };
                                    app.table_structure(row, 0, 0, w, cx);
                                }),
                            ))
                            .child(
                                ui::secondary_button("add-column", "+ Column")
                                    .on_click(cx.listener(|app, _, w, cx| {
                                        let col = match &app.controls {
                                            Some(Controls::Table(f)) => f.aligns.len() - 1,
                                            _ => return,
                                        };
                                        app.table_structure(0, col, 2, w, cx);
                                    })),
                            ),
                    );
            }
        }
        let actions = h_flex()
            .gap_2()
            .child(
                ui::cancel_button("cancel-object")
                    .on_click(cx.listener(|app, _, w, cx| app.close_controls(w, cx))),
            )
            .child(
                ui::primary_button("save-object", "Apply")
                    .on_click(cx.listener(|app, _, w, cx| app.save_controls(w, cx))),
            );
        panel = panel.child(
            h_flex()
                .w_full()
                .justify_between()
                .map(|footer| {
                    if let Controls::Highlight(form) = form {
                        footer.child(
                            ColorPicker::new(&form.picker)
                                .icon(IconName::Palette)
                                .label("Custom")
                                .small()
                                .accessibility_label("Custom highlight color"),
                        )
                    } else {
                        footer.child(div().flex_1())
                    }
                })
                .child(actions),
        );
        Some(
            div()
                .absolute()
                .inset_0()
                .occlude()
                .key_context("RichControls")
                .when_some(modal_focus, |modal, focus| modal.track_focus(&focus))
                .on_action(
                    cx.listener(|app, _: &DismissRichControls, w, cx| app.close_controls(w, cx)),
                )
                .bg(black().opacity(ui::BACKDROP_OPACITY * progress))
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|app, _, w, cx| app.close_controls(w, cx)),
                )
                .child(
                    div()
                        .debug_selector(|| "object-modal-panel".into())
                        .w(px(if matches!(form, Controls::Table(_)) {
                            720.
                        } else {
                            360.
                        }))
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

#[cfg(test)]
mod tests {
    use super::{EditableTable, TableSyntax};

    #[test]
    fn html_tables_open_as_a_grid_and_save_as_html() {
        let source = "<table>\n  <thead>\n    <tr><th>Format</th><th>Extension</th></tr>\n  </thead>\n  <tbody>\n    <tr><td>Markdown</td><td>.md</td></tr>\n    <tr><td>Plain <b>text</b></td><td>.txt</td></tr>\n  </tbody>\n</table>";
        let table = EditableTable::parse(source).unwrap();
        assert_eq!(table.syntax, TableSyntax::Html { header: true });
        let rows: Vec<Vec<String>> = [
            ["Format", "Extension"],
            ["Markdown", ".md"],
            ["Plain <b>text</b>", ".txt"],
        ]
        .iter()
        .map(|row| row.iter().map(|c| c.to_string()).collect())
        .collect();
        assert_eq!(table.rows, rows);
        // Unchanged cells write back the same table.
        assert_eq!(EditableTable::source(&table.rows, &table.aligns, table.syntax), source);
    }

    #[test]
    fn html_tables_without_a_header_keep_td_cells() {
        let table = EditableTable::parse("<TABLE><tr><td>a</td><td>b</td></tr><tr><td>c</td></tr></TABLE>").unwrap();
        assert_eq!(table.syntax, TableSyntax::Html { header: false });
        assert_eq!(table.rows, vec![vec!["a".to_string(), "b".into()], vec!["c".into(), String::new()]]);
        let out = EditableTable::source(&table.rows, &table.aligns, table.syntax);
        assert!(!out.contains("<th>") && out.contains("<tr><td>c</td><td></td></tr>"));
    }

    #[test]
    fn html_tables_the_grid_cannot_hold_are_not_editable() {
        for source in [
            "<table><tr><td colspan=\"2\">a</td></tr></table>",
            "<table><caption>c</caption><tr><td>a</td></tr></table>",
            "<table><tr><td><table><tr><td>x</td></tr></table></td></tr></table>",
            "<table></table>",
            "<div>not a table</div>",
        ] {
            assert_eq!(EditableTable::parse(source), None, "{source}");
        }
    }

    #[test]
    fn markdown_tables_keep_their_alignment_row() {
        let table = EditableTable::parse("| a | b |\n| :-- | --: |\n| 1 | 2 |").unwrap();
        assert_eq!(table.syntax, TableSyntax::Markdown);
        assert_eq!(table.aligns, [":--", "--:"]);
        assert_eq!(table.rows.len(), 2);
    }
    #[test]
    fn image_attributes_and_dimensions_round_trip() {
        let image=super::html_picture("<img src='a&amp;b.png' alt='a &quot;quote&quot;' width='50%' height='200' title='test' />").unwrap();
        assert_eq!(image.src, "a&b.png");
        assert_eq!(image.alt, "a \"quote\"");
        let again = super::html_picture(&image.html()).unwrap();
        assert_eq!(again.alt, image.alt);
        assert_eq!(again.height, "200");
        for invalid in ["0%", "101%", "-1", "NaN", "auto", "1\" onclick=\"bad"] {
            assert!(super::dimension(invalid).is_none());
        }
        for valid in ["50%", "100%", "480", "480px"] {
            assert!(super::dimension(valid).is_some());
        }
    }
    #[test]
    fn highlights_keep_their_body_when_recolored() {
        let source =
            "Before <mark style=\"background-color: #ff008080\">a &amp; <b>b</b></mark> after";
        let cursor = source.find("&amp;").unwrap();
        let (range, body, color) = super::existing_highlight(source, &(cursor..cursor)).unwrap();
        assert_eq!(
            &source[range],
            "<mark style=\"background-color: #ff008080\">a &amp; <b>b</b></mark>"
        );
        assert_eq!(body, "a &amp; <b>b</b>");
        assert_eq!(super::highlight_hex(color), "#ff008080");
        assert!(super::existing_highlight("<marker>text</marker>", &(10..10)).is_none());
    }

    #[test]
    fn image_alignment_round_trips_and_defaults_left() {
        let mut image = super::html_picture("<img src='a.png' width='42%' alt='A' />").unwrap();
        assert_eq!(image.alignment, super::Alignment::Left);
        for alignment in [
            super::Alignment::Center,
            super::Alignment::Right,
            super::Alignment::Left,
        ] {
            image.alignment = alignment;
            let parsed = super::html_picture(&image.html()).unwrap();
            assert_eq!(parsed.alignment, alignment);
            assert_eq!(parsed.width, "42%");
        }
    }
    #[test]
    fn table_edits_preserve_inline_markdown_alignment_and_escaped_pipes() {
        let row = super::source_cells("| **bold** | a\\|b | `code` |");
        assert_eq!(row, vec!["**bold**", "a\\|b", "`code`"]);
        let source = super::table_source(
            &[
                row,
                vec!["new|pipe".into(), "[link](url)".into(), "".into()],
            ],
            &[":---".into(), "---:".into(), ":---:".into()],
        );
        assert!(source.contains("| :--- | ---: | :---: |"));
        assert!(source.contains("a\\|b"));
        assert!(source.contains("new\\|pipe"));
        assert!(source.contains("[link](url)"));
    }
}

#[cfg(all(test, feature = "ui-tests"))]
mod ui_tests {
    use super::{Controls, Picture};
    use crate::{Enso, Mode, bind_view_keys};
    use gpui_kit::{
        AppContext, Bounds, Focusable, MouseMoveEvent, TestAppContext, WindowOptions, point, px,
        size,
    };
    fn setup(
        cx: &mut TestAppContext,
        source: &str,
    ) -> (gpui_kit::AnyWindowHandle, gpui_kit::Entity<Enso>) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_view_keys(cx);
        });
        cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |w, cx| {
                cx.new(|cx| {
                    let mut app = Enso::new(w, cx);
                    app.add_document(None, source.into(), None, w, cx);
                    app
                })
            })
            .unwrap()
        })
    }
    #[gpui_kit::test]
    fn highlight_swatches_stay_present_and_hover_does_not_change_color(cx: &mut TestAppContext) {
        let source = "red (hex with alpha)";
        let (handle, view) = setup(cx, source);
        let editor = cx.update(|cx| view.read(cx).documents[0].editor.clone());
        cx.update_window(handle, |_, w, cx| {
            view.update(cx, |app, cx| {
                editor.update(cx, |e, cx| e.set_selected_range(0..source.len(), cx));
                app.open_highlight(editor.clone(), false, w, cx);
            })
        })
        .unwrap();
        let draw = |cx: &mut TestAppContext| {
            cx.update_window(handle, |_, w, cx| {
                w.refresh();
                w.draw(cx).clear(cx);
            })
            .unwrap();
            cx.run_until_parked();
        };
        draw(cx);
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(200));
        draw(cx);
        let mut visual = gpui_kit::VisualTestContext::from_window(handle, cx);
        assert!(
            visual
                .debug_bounds("object-modal-panel")
                .unwrap()
                .size
                .height
                < px(180.),
            "color chooser should stay compact"
        );
        for selector in [
            "highlight-swatch-0",
            "highlight-swatch-1",
            "highlight-swatch-2",
            "highlight-swatch-3",
            "highlight-swatch-4",
            "highlight-swatch-5",
            "highlight-swatch-6",
        ] {
            let before = visual.debug_bounds(selector).unwrap();
            visual.simulate_mouse_move(before.center(), None, gpui_kit::Modifiers::default());
            draw(cx);
            assert_eq!(
                visual.debug_bounds(selector).unwrap(),
                before,
                "hover must not shift or remove a swatch"
            );
            cx.update(|cx| {
                if let Some(Controls::Highlight(form)) = &view.read(cx).controls {
                    assert_eq!(
                        super::highlight_hex(form.picker.read(cx).value().unwrap()),
                        "#fef08a"
                    );
                } else {
                    panic!("hover closed the chooser");
                }
                assert_eq!(editor.read(cx).value().as_str(), source);
            });
        }
        let mint = visual.debug_bounds("highlight-swatch-6").unwrap();
        visual.simulate_click(mint.center(), gpui_kit::Modifiers::default());
        draw(cx);
        cx.update(|cx| {
            if let Some(Controls::Highlight(form)) = &view.read(cx).controls {
                assert_eq!(
                    super::highlight_hex(form.picker.read(cx).value().unwrap()),
                    "#bbf7d0"
                );
            }
        });
    }

    #[gpui_kit::test]
    fn highlight_picker_applies_recolors_and_cancels(cx: &mut TestAppContext) {
        let source = "a & b";
        let (handle, view) = setup(cx, source);
        let editor = cx.update(|cx| view.read(cx).documents[0].editor.clone());
        cx.update_window(handle, |_, w, cx| {
            view.update(cx, |app, cx| app.set_mode(Mode::Split, w, cx))
        })
        .unwrap();
        cx.update_window(handle, |_, w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, w, cx| {
            view.update(cx, |app, cx| {
                editor.update(cx, |e, cx| e.set_selected_range(0..source.len(), cx));
                editor.focus_handle(cx).focus(w, cx);
                app.open_highlight(editor.clone(), false, w, cx);
                assert_eq!(editor.read(cx).value().as_str(), source);
                app.choose_highlight_color(gpui_kit::rgb(0xfbcfe8).into(), w, cx);
                app.save_controls(w, cx);
                assert_eq!(
                    editor.read(cx).value().as_str(),
                    "<mark color=\"#fbcfe8\">a &amp; b</mark>"
                );
                editor.update(cx, |e, cx| e.set_selected_range(24..24, cx));
                app.open_highlight(editor.clone(), false, w, cx);
                if let Some(Controls::Highlight(form)) = &app.controls {
                    assert_eq!(
                        super::highlight_hex(form.picker.read(cx).value().unwrap()),
                        "#fbcfe8"
                    );
                } else {
                    panic!("highlight picker did not open");
                }
                app.choose_highlight_color(gpui_kit::rgb(0xbbf7d0).into(), w, cx);
                app.save_controls(w, cx);
                assert_eq!(
                    editor.read(cx).value().as_str(),
                    "<mark color=\"#bbf7d0\">a &amp; b</mark>"
                );
                editor.update(cx, |e, cx| e.set_selected_range(24..24, cx));
                app.open_highlight(editor.clone(), false, w, cx);
                app.choose_highlight_color(gpui_kit::rgb(0xbfdbfe).into(), w, cx);
            })
        })
        .unwrap();
        cx.update_window(handle, |_, w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.simulate_keystrokes(handle, "escape");
        cx.update(|cx| {
            assert!(view.read(cx).controls.is_none());
            assert_eq!(
                editor.read(cx).value().as_str(),
                "<mark color=\"#bbf7d0\">a &amp; b</mark>"
            );
        });
    }

    #[gpui_kit::test]
    fn modals_slide_down_and_keep_their_surface_until_closed(cx: &mut TestAppContext) {
        let source = "![old](image.png)";
        let (handle, view) = setup(cx, source);
        let draw = |cx: &mut TestAppContext| {
            cx.update_window(handle, |_, w, cx| {
                w.refresh();
                w.draw(cx).clear(cx);
            })
            .unwrap();
            cx.run_until_parked();
        };
        draw(cx);
        cx.update_window(handle, |_, w, cx| {
            view.update(cx, |app, cx| {
                let id = app.documents[0].editor.entity_id();
                app.open_picture(
                    id,
                    0..source.len(),
                    source.into(),
                    Picture {
                        src: "image.png".into(),
                        alt: "old".into(),
                        width: "100%".into(),
                        ..Default::default()
                    },
                    w,
                    cx,
                );
            })
        })
        .unwrap();
        draw(cx);
        let mut visual = gpui_kit::VisualTestContext::from_window(handle, cx);
        let start = visual.debug_bounds("object-modal-panel").unwrap().top();
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(90));
        draw(cx);
        let middle = visual.debug_bounds("object-modal-panel").unwrap().top();
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(100));
        draw(cx);
        let end = visual.debug_bounds("object-modal-panel").unwrap().top();
        assert!(
            middle > start && end >= middle,
            "modal should move downward: {start:?} {middle:?} {end:?}"
        );
        cx.update_window(handle, |_, w, cx| {
            view.update(cx, |app, cx| app.close_controls(w, cx))
        })
        .unwrap();
        draw(cx);
        cx.update(|cx| {
            assert!(view.read(cx).controls.is_none());
            assert!(view.read(cx).closing_controls.is_some());
        });
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(90));
        draw(cx);
        assert!(
            cx.update(|cx| view.read(cx).closing_controls.is_some()),
            "exit surface should remain mid-fade"
        );
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(100));
        draw(cx);
        assert!(cx.update(|cx| view.read(cx).closing_controls.is_none()));
    }

    #[gpui_kit::test]
    fn rendered_checkboxes_toggle_source_in_read_mode(cx: &mut TestAppContext) {
        let source = "- [ ] **Task**\n- [x] Done\n  - [ ] Nested\n";
        let (handle, view) = setup(cx, source);
        let mut visual = gpui_kit::VisualTestContext::from_window(handle, cx);
        for expected in ["- [x] **Task**", "- [ ] **Task**"] {
            cx.update_window(handle, |_, w, cx| {
                w.refresh();
                w.draw(cx).clear(cx);
            })
            .unwrap();
            cx.run_until_parked();
            let bounds = visual
                .debug_bounds("task-checkbox-2")
                .expect("checkbox rendered");
            visual.simulate_click(bounds.center(), gpui_kit::Modifiers::default());
            cx.run_until_parked();
            cx.update(|cx| {
                let app = view.read(cx);
                assert!(app.mode == crate::Mode::Read);
                assert!(
                    app.documents[0]
                        .editor
                        .read(cx)
                        .value()
                        .starts_with(expected)
                );
            });
        }
    }

    #[gpui_kit::test]
    fn resized_image_layout_follows_its_aspect_ratio(cx: &mut TestAppContext) {
        let path =
            std::env::temp_dir().join(format!("enso-image-layout-{}.png", std::process::id()));
        std::fs::write(&path, include_bytes!("../assets/logo-dark.png")).unwrap();
        let source = format!(
            "Before\n\n<img src=\"{}\" alt=\"logo\" width=\"50%\" />\n\nAfter",
            path.display()
        );
        let (handle, view) = setup(cx, &source);
        let mut visual = gpui_kit::VisualTestContext::from_window(handle, cx);
        for _ in 0..3 {
            cx.update_window(handle, |_, w, cx| {
                w.refresh();
                w.draw(cx).clear(cx);
            })
            .unwrap();
            cx.run_until_parked();
        }
        let bounds = visual
            .debug_bounds("image-frame")
            .expect("image frame rendered");
        assert!(bounds.size.width > px(20.), "nonzero image width");
        assert!(
            (f32::from(bounds.size.width) - f32::from(bounds.size.height)).abs() < 2.,
            "square image must have a square layout box, got {bounds:?}"
        );
        visual.simulate_click(bounds.center(), gpui_kit::Modifiers::default());
        assert!(
            cx.update(|cx| view.read(cx).controls.is_none()),
            "image click must not open the editor"
        );
        let handle_bounds = visual
            .debug_bounds("image-handle-right")
            .expect("resize handle");
        visual.simulate_mouse_move(handle_bounds.center(), None, gpui_kit::Modifiers::default());
        visual.simulate_mouse_down(
            handle_bounds.center(),
            gpui_kit::MouseButton::Left,
            gpui_kit::Modifiers::default(),
        );
        let destination = point(bounds.left() + bounds.size.width * 0.8, bounds.center().y);
        visual.simulate_mouse_move(
            destination,
            Some(gpui_kit::MouseButton::Left),
            gpui_kit::Modifiers::default(),
        );
        visual.simulate_mouse_up(
            destination,
            gpui_kit::MouseButton::Left,
            gpui_kit::Modifiers::default(),
        );
        cx.run_until_parked();
        cx.update(|cx| {
            assert!(
                view.read(cx).documents[0]
                    .editor
                    .read(cx)
                    .value()
                    .contains("width=\"40.0%\""),
                "{}",
                view.read(cx).documents[0].editor.read(cx).value()
            )
        });
        for alignment in ["center", "right", "left"] {
            cx.update_window(handle, |_, w, cx| {
                w.refresh();
                w.draw(cx).clear(cx);
            })
            .unwrap();
            cx.run_until_parked();
            let selector = match alignment {
                "center" => "image-align-center",
                "right" => "image-align-right",
                _ => "image-align-left",
            };
            let button = visual
                .debug_bounds(selector)
                .expect("alignment button rendered");
            visual.simulate_mouse_move(button.center(), None, gpui_kit::Modifiers::default());
            visual.simulate_click(button.center(), gpui_kit::Modifiers::default());
            cx.update_window(handle, |_, w, cx| {
                w.refresh();
                w.draw(cx).clear(cx);
            })
            .unwrap();
            cx.run_until_parked();
            let frame = visual.debug_bounds("image-frame").unwrap();
            let available = visual.debug_bounds("image-available").unwrap();
            let delta = match alignment {
                "center" => frame.center().x - available.center().x,
                "right" => frame.right() - available.right(),
                _ => frame.left() - available.left(),
            };
            assert!(
                f32::from(delta).abs() < 2.,
                "{alignment} alignment is incorrect: {frame:?} within {available:?}"
            );
            assert!(cx.update(|cx| view.read(cx).controls.is_none()));
        }
        let edit = visual.debug_bounds("image-edit-button").unwrap();
        visual.simulate_mouse_move(edit.center(), None, gpui_kit::Modifiers::default());
        visual.simulate_click(edit.center(), gpui_kit::Modifiers::default());
        assert!(
            cx.update(|cx| view.read(cx).controls.is_some()),
            "Edit button opens image controls"
        );
        std::fs::remove_file(path).unwrap();
    }

    #[gpui_kit::test]
    fn image_cancel_preserves_source_and_resize_is_one_change(cx: &mut TestAppContext) {
        let source = "![old](image.png)";
        let (handle, view) = setup(cx, source);
        let editor = cx.update(|cx| view.read(cx).documents[0].editor.clone());
        cx.update_window(handle, |_, w, cx| {
            view.update(cx, |app, cx| {
                let picture = Picture {
                    src: "image.png".into(),
                    alt: "old".into(),
                    width: "100%".into(),
                    ..Default::default()
                };
                app.open_picture(
                    editor.entity_id(),
                    0..source.len(),
                    source.into(),
                    picture.clone(),
                    w,
                    cx,
                );
                if let Some(Controls::Image(form)) = &app.controls {
                    form.alt
                        .update(cx, |e, cx| e.set_value("changed alt text", w, cx));
                }
                app.close_controls(w, cx);
                assert_eq!(
                    editor.read(cx).value().as_str(),
                    source,
                    "cancel preserves the image"
                );
                app.start_image_resize(
                    editor.entity_id(),
                    0..source.len(),
                    source.into(),
                    picture,
                    Bounds::new(point(px(10.), px(10.)), size(px(200.), px(100.))),
                    px(400.),
                    false,
                    w,
                    cx,
                );
                app.move_image_resize(
                    &MouseMoveEvent {
                        position: point(px(310.), px(20.)),
                        ..Default::default()
                    },
                    cx,
                );
                assert_eq!(
                    editor.read(cx).value().as_str(),
                    source,
                    "drag preview must not write source"
                );
                app.finish_image_resize(w, cx);
                assert!(editor.read(cx).value().contains("width=\"75.0%\""));
                editor.focus_handle(cx).focus(w, cx);
            })
        })
        .unwrap();
        cx.update_window(handle, |_, w, cx| {
            view.update(cx, |app, cx| app.set_mode(Mode::Edit, w, cx))
        })
        .unwrap();
        cx.update_window(handle, |_, w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.simulate_keystrokes(
            handle,
            if cfg!(target_os = "macos") {
                "cmd-z"
            } else {
                "ctrl-z"
            },
        );
        assert_eq!(cx.update(|cx| editor.read(cx).value().to_string()), source);
    }
    #[gpui_kit::test]
    fn table_dot_menu_removes_rows_and_columns(cx: &mut TestAppContext) {
        let source = "| A | B |\n| --- | --- |\n| remove | row |\n| keep | this |";
        let (handle, view) = setup(cx, source);
        cx.update_window(handle, |_, w, cx| {
            view.update(cx, |app, cx| {
                app.open_table(app.documents[0].editor.entity_id(), 0..source.len(), w, cx);
            })
        })
        .unwrap();
        let draw = |cx: &mut TestAppContext| {
            cx.update_window(handle, |_, w, cx| {
                w.refresh();
                w.draw(cx).clear(cx);
            })
            .unwrap();
            cx.run_until_parked();
        };
        draw(cx);
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(200));
        draw(cx);
        let mut visual = gpui_kit::VisualTestContext::from_window(handle, cx);
        let button = visual
            .debug_bounds("table-menu-1-0")
            .expect("row menu button");
        visual.simulate_mouse_move(button.center(), None, gpui_kit::Modifiers::default());
        draw(cx);
        visual.simulate_click(button.center(), gpui_kit::Modifiers::default());
        draw(cx);
        cx.simulate_keystrokes(handle, "down down enter");
        draw(cx);
        cx.update(|cx| match &view.read(cx).controls {
            Some(Controls::Table(f)) => {
                assert_eq!(f.cells.len(), 2);
                assert_eq!(f.cells[1][0].read(cx).value().as_str(), "keep");
            }
            _ => panic!("table closed unexpectedly"),
        });
        let button = visual
            .debug_bounds("table-menu-1-1")
            .expect("column menu button");
        visual.simulate_mouse_move(button.center(), None, gpui_kit::Modifiers::default());
        draw(cx);
        visual.simulate_click(button.center(), gpui_kit::Modifiers::default());
        draw(cx);
        cx.simulate_keystrokes(handle, "down down down down enter");
        draw(cx);
        cx.update(|cx| match &view.read(cx).controls {
            Some(Controls::Table(f)) => {
                assert_eq!(f.aligns.len(), 1);
                assert!(f.cells.iter().all(|row| row.len() == 1));
            }
            _ => panic!("table closed unexpectedly"),
        });
    }

    #[gpui_kit::test]
    fn table_grid_tabs_and_applies_without_losing_markup(cx: &mut TestAppContext) {
        let source = "| **A** | B |\n| :--- | ---: |\n| old | [link](url) |";
        let (handle, view) = setup(cx, source);
        let editor = cx.update(|cx| view.read(cx).documents[0].editor.clone());
        cx.update_window(handle, |_, w, cx| {
            view.update(cx, |app, cx| {
                app.open_table(editor.entity_id(), 0..source.len(), w, cx)
            })
        })
        .unwrap();
        cx.update_window(handle, |_, w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.simulate_keystrokes(handle, "tab");
        cx.update_window(handle, |_, w, cx| {
            view.update(cx, |app, cx| {
                if let Some(Controls::Table(form)) = &app.controls {
                    assert!(form.cells[0][1].focus_handle(cx).is_focused(w));
                    form.cells[1][0].update(cx, |e, cx| e.set_value("new|value", w, cx));
                }
                app.table_structure(1, 0, 0, w, cx);
                app.table_structure(0, 1, 2, w, cx);
                app.table_structure(0, 2, 3, w, cx);
                app.save_controls(w, cx);
                let text = editor.read(cx).value();
                assert!(text.contains("**A**"));
                assert!(text.contains("new\\|value"));
                assert!(text.contains("[link](url)"));
                assert!(text.contains(":--- | ---:"));
                assert_eq!(text.lines().count(), 4);
            })
        })
        .unwrap();
    }
}
