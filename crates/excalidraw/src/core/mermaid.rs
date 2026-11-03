use std::collections::HashMap;

use crate::{
    arrowhead::Arrowhead,
    binding::{focus_point, make_binding, refresh_derived},
    bounds::common_bounds,
    element::{BoundElement, BoundElementType, Element},
    factory::{ElementOptions, new_arrow, new_diamond, new_ellipse, new_rectangle, new_text},
    geometry::Point,
    scene::Scene,
    text::{text_height, text_width},
    transform::translate_element,
    types::{FillStyle, Roundness, RoundnessType, StrokeStyle, TextAlign, VerticalAlign},
};

/// Font size used for the labels of the generated diagram.
pub const MERMAID_FONT_SIZE: f64 = 20.0;

const LINE_HEIGHT: f64 = 1.25;
const NODE_GAP: f64 = 50.0;
/// Rank separation leaves room for a one-line edge label plus the arrow.
const RANK_GAP: f64 = 70.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    TopDown,
    BottomUp,
    LeftRight,
    RightLeft,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeShape {
    Rectangle,
    Rounded,
    Stadium,
    Diamond,
    Circle,
    Hexagon,
    Cylinder,
    Subroutine,
    Parallelogram,
}

#[derive(Debug, Clone)]
struct Node {
    id: String,
    label: String,
    shape: NodeShape,
}

#[derive(Debug, Clone)]
struct Edge {
    from: String,
    to: String,
    label: Option<String>,
    arrow: bool,
    dashed: bool,
}

#[derive(Debug, Clone, Default)]
struct Flowchart {
    direction: Option<Direction>,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
}

#[derive(Debug, Clone, Copy)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// Parses a Mermaid flowchart and returns Excalidraw elements laid out from the
/// top-left corner of the diagram.
pub fn mermaid_elements(source: &str) -> Result<Vec<Element>, String> {
    let flowchart = parse_flowchart(source)?;
    if flowchart.nodes.is_empty() {
        return Err("No nodes found in the diagram".to_string());
    }

    let positions = layout(&flowchart);
    let options = ElementOptions {
        background_color: "transparent".to_string(),
        fill_style: FillStyle::Solid,
        ..ElementOptions::default()
    };

    let mut scene = Scene::new();
    let mut containers: HashMap<String, Element> = HashMap::new();

    for node in &flowchart.nodes {
        let rect = positions[&node.id];
        let mut container = match node.shape {
            NodeShape::Diamond => new_diamond(rect.x, rect.y, rect.w, rect.h, &options),
            NodeShape::Circle => new_ellipse(rect.x, rect.y, rect.w, rect.h, &options),
            _ => new_rectangle(rect.x, rect.y, rect.w, rect.h, &options),
        };
        if let Some(base) = container_base_mut(&mut container) {
            base.roundness = match node.shape {
                NodeShape::Rounded | NodeShape::Stadium => Some(Roundness {
                    kind: RoundnessType::AdaptiveRadius,
                    value: None,
                }),
                _ => None,
            };
        }

        let mut text = new_text(rect.x, rect.y, &node.label, MERMAID_FONT_SIZE, &options);
        let text_id = text.id().to_string();
        if let Element::Text(t) = &mut text {
            t.container_id = Some(container.id().to_string());
            t.text_align = TextAlign::Center;
            t.vertical_align = VerticalAlign::Middle;
        }
        if let Some(base) = container_base_mut(&mut container) {
            base.bound_elements = Some(vec![BoundElement {
                id: text_id,
                kind: BoundElementType::Text,
            }]);
        }

        containers.insert(node.id.clone(), container.clone());
        scene.add(container);
        scene.add(text);
    }

    for edge in &flowchart.edges {
        let (Some(from), Some(to)) = (positions.get(&edge.from), positions.get(&edge.to)) else {
            continue;
        };
        let (Some(from_element), Some(to_element)) =
            (containers.get(&edge.from), containers.get(&edge.to))
        else {
            continue;
        };

        let start = focus_point(from_element, center_of(to));
        let end = focus_point(to_element, center_of(from));
        let mut arrow = new_arrow(
            vec![start, end],
            None,
            edge.arrow.then_some(Arrowhead::Arrow),
            &options,
        );
        let arrow_id = arrow.id().to_string();
        if let Element::Arrow(a) = &mut arrow {
            if edge.dashed {
                a.linear.base.stroke_style = StrokeStyle::Dashed;
            }
            a.linear.start_binding = Some(make_binding(from_element, start));
            a.linear.end_binding = Some(make_binding(to_element, end));
        }

        let mut label = None;
        if let Some(text) = &edge.label {
            let width = text_width(text, MERMAID_FONT_SIZE);
            let height = text_height(text, MERMAID_FONT_SIZE, LINE_HEIGHT);
            let mid = Point::new((start.x + end.x) / 2.0, (start.y + end.y) / 2.0);
            let mut element = new_text(
                mid.x - width / 2.0,
                mid.y - height / 2.0,
                text,
                MERMAID_FONT_SIZE,
                &options,
            );
            let text_id = element.id().to_string();
            if let Element::Text(t) = &mut element {
                t.base.width = width;
                t.base.height = height;
                t.container_id = Some(arrow_id.clone());
                t.text_align = TextAlign::Center;
                t.vertical_align = VerticalAlign::Middle;
            }
            if let Element::Arrow(a) = &mut arrow {
                a.linear.base.bound_elements = Some(vec![BoundElement {
                    id: text_id,
                    kind: BoundElementType::Text,
                }]);
            }
            label = Some(element);
        }

        scene.add(arrow);
        if let Some(label) = label {
            scene.add(label);
        }
    }

    refresh_derived(&mut scene);

    let mut elements: Vec<Element> = scene.non_deleted().cloned().collect();
    let refs: Vec<&Element> = elements.iter().collect();
    if let Some(bounds) = common_bounds(&refs) {
        for element in &mut elements {
            translate_element(element, -bounds.min_x, -bounds.min_y);
        }
    }
    Ok(elements)
}

fn container_base_mut(element: &mut Element) -> Option<&mut crate::element::ElementBase> {
    match element {
        Element::Rectangle { base } | Element::Diamond { base } | Element::Ellipse { base } => {
            Some(base)
        }
        _ => None,
    }
}

fn center_of(rect: &Rect) -> Point {
    Point::new(rect.x + rect.w / 2.0, rect.y + rect.h / 2.0)
}

fn node_size(node: &Node) -> (f64, f64) {
    let width = text_width(&node.label, MERMAID_FONT_SIZE).max(MERMAID_FONT_SIZE);
    let height = text_height(&node.label, MERMAID_FONT_SIZE, LINE_HEIGHT).max(MERMAID_FONT_SIZE);
    match node.shape {
        NodeShape::Diamond => (width * 1.35 + 30.0, height * 1.3 + 30.0),
        NodeShape::Circle => {
            let side = (width + 30.0).max(height + 30.0);
            (side, side)
        }
        _ => (width + 30.0, (height + 22.0).max(40.0)),
    }
}

fn layout(flowchart: &Flowchart) -> HashMap<String, Rect> {
    let index: HashMap<&str, usize> = flowchart
        .nodes
        .iter()
        .enumerate()
        .map(|(i, node)| (node.id.as_str(), i))
        .collect();
    let count = flowchart.nodes.len();
    let mut parents: Vec<Vec<usize>> = vec![Vec::new(); count];
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); count];
    for edge in &flowchart.edges {
        let (Some(&from), Some(&to)) = (index.get(edge.from.as_str()), index.get(edge.to.as_str()))
        else {
            continue;
        };
        if from == to {
            continue;
        }
        if !parents[to].contains(&from) {
            parents[to].push(from);
            children[from].push(to);
        }
    }

    let ranks = rank_nodes(count, &parents, &children);
    let mut layers: Vec<Vec<usize>> =
        vec![Vec::new(); ranks.iter().copied().max().unwrap_or(0) + 1];
    for i in 0..count {
        layers[ranks[i]].push(i);
    }
    order_layers(&mut layers, &parents, &children);

    let sizes: Vec<(f64, f64)> = flowchart.nodes.iter().map(node_size).collect();
    let vertical = matches!(
        flowchart.direction.unwrap_or(Direction::TopDown),
        Direction::TopDown | Direction::BottomUp
    );

    let mut primary_cursor = 0.0;
    let mut max_cross: f64 = 0.0;
    let mut layer_primary: Vec<f64> = Vec::with_capacity(layers.len());
    for layer in &layers {
        let primary = layer
            .iter()
            .map(|&i| if vertical { sizes[i].1 } else { sizes[i].0 })
            .fold(0.0, f64::max);
        let cross: f64 = layer
            .iter()
            .map(|&i| if vertical { sizes[i].0 } else { sizes[i].1 })
            .sum::<f64>()
            + NODE_GAP * (layer.len().saturating_sub(1)) as f64;
        max_cross = max_cross.max(cross);
        layer_primary.push(primary);
    }

    let mut rects = vec![
        Rect {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0
        };
        count
    ];
    for (rank, layer) in layers.iter().enumerate() {
        let primary = layer_primary[rank];
        let cross: f64 = layer
            .iter()
            .map(|&i| if vertical { sizes[i].0 } else { sizes[i].1 })
            .sum::<f64>()
            + NODE_GAP * (layer.len().saturating_sub(1)) as f64;
        let mut cross_cursor = (max_cross - cross) / 2.0;
        for &i in layer {
            let (w, h) = sizes[i];
            let (primary_pos, cross_pos) = if vertical {
                (primary_cursor + (primary - h) / 2.0, cross_cursor)
            } else {
                (primary_cursor + (primary - w) / 2.0, cross_cursor)
            };
            rects[i] = Rect {
                x: if vertical { cross_pos } else { primary_pos },
                y: if vertical { primary_pos } else { cross_pos },
                w,
                h,
            };
            cross_cursor += if vertical { w } else { h } + NODE_GAP;
        }
        primary_cursor += primary + RANK_GAP;
    }

    let total_primary = (primary_cursor - RANK_GAP).max(0.0);
    match flowchart.direction.unwrap_or(Direction::TopDown) {
        Direction::BottomUp => {
            for rect in &mut rects {
                rect.y = total_primary - rect.y - rect.h;
            }
        }
        Direction::RightLeft => {
            for rect in &mut rects {
                rect.x = total_primary - rect.x - rect.w;
            }
        }
        _ => {}
    }

    flowchart
        .nodes
        .iter()
        .enumerate()
        .map(|(i, node)| (node.id.clone(), rects[i]))
        .collect()
}

fn rank_nodes(count: usize, parents: &[Vec<usize>], children: &[Vec<usize>]) -> Vec<usize> {
    let mut indegree: Vec<usize> = parents.iter().map(Vec::len).collect();
    let mut ranks = vec![0usize; count];
    let mut queue: Vec<usize> = (0..count).filter(|i| indegree[*i] == 0).collect();
    let mut processed = 0;
    while let Some(node) = queue.pop() {
        processed += 1;
        for &child in &children[node] {
            ranks[child] = ranks[child].max(ranks[node] + 1);
            indegree[child] -= 1;
            if indegree[child] == 0 {
                queue.push(child);
            }
        }
    }
    if processed < count {
        // Nodes inside a cycle never reach indegree 0; stack them below their
        // known ancestors so the diagram still renders.
        let mut next_rank = ranks.iter().copied().max().unwrap_or(0) + 1;
        for node in 0..count {
            if indegree[node] > 0 {
                ranks[node] = next_rank;
                next_rank += 1;
            }
        }
    }
    ranks
}

fn order_layers(layers: &mut [Vec<usize>], parents: &[Vec<usize>], children: &[Vec<usize>]) {
    for _ in 0..4 {
        for rank in 1..layers.len() {
            let positions = layer_positions(&layers[rank - 1]);
            let mut layer = std::mem::take(&mut layers[rank]);
            sort_layer(&mut layer, parents, &positions);
            layers[rank] = layer;
        }
        for rank in (0..layers.len().saturating_sub(1)).rev() {
            let positions = layer_positions(&layers[rank + 1]);
            let mut layer = std::mem::take(&mut layers[rank]);
            sort_layer(&mut layer, children, &positions);
            layers[rank] = layer;
        }
    }
}

fn layer_positions(layer: &[usize]) -> HashMap<usize, f64> {
    layer
        .iter()
        .enumerate()
        .map(|(position, id)| (*id, position as f64))
        .collect()
}

fn sort_layer(layer: &mut [usize], neighbors: &[Vec<usize>], positions: &HashMap<usize, f64>) {
    layer.sort_by(|a, b| {
        barycenter(*a, neighbors, positions)
            .partial_cmp(&barycenter(*b, neighbors, positions))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.cmp(b))
    });
}

fn barycenter(node: usize, neighbors: &[Vec<usize>], positions: &HashMap<usize, f64>) -> f64 {
    let values: Vec<f64> = neighbors[node]
        .iter()
        .filter_map(|neighbor| positions.get(neighbor).copied())
        .collect();
    if values.is_empty() {
        f64::MAX
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn parse_flowchart(source: &str) -> Result<Flowchart, String> {
    let mut flowchart = Flowchart::default();
    let mut index: HashMap<String, usize> = HashMap::new();

    for raw_line in source.lines() {
        let line = raw_line.split("%%").next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        for statement in line.split(';') {
            let statement = statement.trim();
            if statement.is_empty() {
                continue;
            }
            if flowchart.direction.is_none() {
                let mut words = statement.split_whitespace();
                let head = words.next().unwrap_or("");
                if matches!(head, "flowchart" | "graph") {
                    let direction = words.next().unwrap_or("TD");
                    flowchart.direction = Some(parse_direction(direction)?);
                    let rest = statement[head.len()..].trim_start();
                    let rest = rest.strip_prefix(direction).unwrap_or("").trim();
                    if !rest.is_empty() {
                        parse_statement(rest, &mut flowchart, &mut index)?;
                    }
                    continue;
                }
                if is_unsupported_diagram(head) {
                    return Err(format!("The {head} diagram type is not supported yet"));
                }
                return Err(
                    "No diagram type detected. Start with `flowchart` or `graph`.".to_string(),
                );
            }
            if is_directive(statement) {
                continue;
            }
            parse_statement(statement, &mut flowchart, &mut index)?;
        }
    }

    if flowchart.direction.is_none() {
        return Err("No diagram type detected. Start with `flowchart` or `graph`.".to_string());
    }
    Ok(flowchart)
}

fn is_unsupported_diagram(head: &str) -> bool {
    matches!(
        head,
        "sequenceDiagram"
            | "classDiagram"
            | "classDiagram-v2"
            | "erDiagram"
            | "stateDiagram"
            | "stateDiagram-v2"
            | "journey"
            | "gantt"
            | "pie"
            | "mindmap"
            | "timeline"
            | "gitGraph"
            | "quadrantChart"
            | "requirementDiagram"
            | "C4Context"
    )
}

fn is_directive(statement: &str) -> bool {
    let head = statement.split_whitespace().next().unwrap_or("");
    matches!(
        head,
        "subgraph"
            | "end"
            | "style"
            | "classDef"
            | "class"
            | "click"
            | "linkStyle"
            | "direction"
            | "accTitle"
            | "accDescr"
            | "title"
    )
}

fn parse_direction(raw: &str) -> Result<Direction, String> {
    match raw {
        "TD" | "TB" => Ok(Direction::TopDown),
        "BT" => Ok(Direction::BottomUp),
        "LR" => Ok(Direction::LeftRight),
        "RL" => Ok(Direction::RightLeft),
        other => Err(format!("Unknown flowchart direction: {other}")),
    }
}

fn parse_statement(
    statement: &str,
    flowchart: &mut Flowchart,
    index: &mut HashMap<String, usize>,
) -> Result<(), String> {
    let mut cursor = Cursor::new(statement);
    let mut previous = parse_node_ref(&mut cursor, flowchart, index)?;
    loop {
        cursor.skip_whitespace();
        if cursor.at_end() {
            break;
        }
        let link = parse_link(&mut cursor)?;
        let next = parse_node_ref(&mut cursor, flowchart, index)?;
        flowchart.edges.push(Edge {
            from: previous,
            to: next.clone(),
            label: link.label,
            arrow: link.arrow,
            dashed: link.dashed,
        });
        previous = next;
    }
    Ok(())
}

fn parse_node_ref(
    cursor: &mut Cursor,
    flowchart: &mut Flowchart,
    index: &mut HashMap<String, usize>,
) -> Result<String, String> {
    cursor.skip_whitespace();
    let start = cursor.position;
    while let Some(ch) = cursor.peek() {
        if ch.is_alphanumeric() || ch == '_' {
            cursor.bump();
        } else {
            break;
        }
    }
    if cursor.position == start {
        return Err(format!("Unexpected input near `{}`", cursor.rest()));
    }
    let id = cursor.text[start..cursor.position].to_string();
    let declared = parse_shape(cursor)?;
    match index.get(&id) {
        Some(&existing) => {
            if let Some((shape, label)) = declared {
                let node = &mut flowchart.nodes[existing];
                node.shape = shape;
                if let Some(label) = label {
                    node.label = label;
                }
            }
        }
        None => {
            let (shape, label) = declared.unwrap_or((NodeShape::Rectangle, None));
            let label = label.unwrap_or_else(|| id.clone());
            index.insert(id.clone(), flowchart.nodes.len());
            flowchart.nodes.push(Node {
                id: id.clone(),
                label,
                shape,
            });
        }
    }
    Ok(id)
}

fn parse_shape(cursor: &mut Cursor) -> Result<Option<(NodeShape, Option<String>)>, String> {
    let rest = cursor.rest();
    let (shape, open, close) = if rest.starts_with("((") {
        (NodeShape::Circle, "((", "))")
    } else if rest.starts_with("([") {
        (NodeShape::Stadium, "([", "])")
    } else if rest.starts_with("[[") {
        (NodeShape::Subroutine, "[[", "]]")
    } else if rest.starts_with("[(") {
        (NodeShape::Cylinder, "[(", ")]")
    } else if rest.starts_with("{{") {
        (NodeShape::Hexagon, "{{", "}}")
    } else if rest.starts_with("[/") {
        (NodeShape::Parallelogram, "[/", "/]")
    } else if rest.starts_with("[\\") {
        (NodeShape::Parallelogram, "[\\", "\\]")
    } else if rest.starts_with('[') {
        (NodeShape::Rectangle, "[", "]")
    } else if rest.starts_with('(') {
        (NodeShape::Rounded, "(", ")")
    } else if rest.starts_with('{') {
        (NodeShape::Diamond, "{", "}")
    } else if rest.starts_with('>') {
        (NodeShape::Rectangle, ">", "]")
    } else {
        return Ok(None);
    };
    cursor.advance(open.len());
    let Some(end) = cursor.rest().find(close) else {
        return Err(format!("Unclosed node label near `{}`", cursor.rest()));
    };
    let label = clean_label(&cursor.rest()[..end]);
    cursor.advance(end + close.len());
    Ok(Some((shape, Some(label))))
}

fn clean_label(raw: &str) -> String {
    let trimmed = raw.trim();
    let unquoted = if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    };
    unquoted
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("<br>", "\n")
        .replace("**", "")
        .trim()
        .to_string()
}

#[derive(Debug, Clone)]
struct Link {
    arrow: bool,
    dashed: bool,
    label: Option<String>,
}

fn parse_link(cursor: &mut Cursor) -> Result<Link, String> {
    cursor.skip_whitespace();
    cursor.advance_while('<');
    let rest = cursor.rest();
    let (marker, dashed) = if rest.starts_with("-.") {
        ("-.", true)
    } else if rest.starts_with("==") {
        ("==", false)
    } else if rest.starts_with("--") {
        ("--", false)
    } else {
        return Err(format!("Expected a link near `{}`", rest));
    };
    cursor.advance(marker.len());

    let mut arrow = true;
    let mut label = None;
    let rest = cursor.rest();
    if rest.starts_with('>') {
        cursor.advance(1);
    } else if dashed && rest.starts_with("->") {
        cursor.advance(2);
    } else if !dashed && rest.starts_with('-') {
        cursor.advance_while('-');
        arrow = cursor.eat(">");
    } else if !dashed && (rest.starts_with('o') || rest.starts_with('x')) {
        cursor.advance(1);
    } else if !dashed && rest.starts_with('=') {
        cursor.advance_while('=');
        arrow = cursor.eat(">");
    } else {
        let terminators: &[(&str, bool)] = if dashed {
            &[(".->", true), (".-", false)]
        } else if marker == "==" {
            &[("==>", true), ("===", false)]
        } else {
            &[("-->", true), ("---", false)]
        };
        let Some((offset, text, arrow_head)) = find_terminator(cursor.rest(), terminators) else {
            return Err(format!("Unclosed link near `{}`", cursor.rest()));
        };
        label = Some(clean_label(&cursor.rest()[..offset]));
        cursor.advance(offset + text.len());
        arrow = arrow_head;
    }

    cursor.skip_whitespace();
    if cursor.peek() == Some('|') {
        cursor.advance(1);
        let Some(end) = cursor.rest().find('|') else {
            return Err("Unclosed edge label".to_string());
        };
        label = Some(clean_label(&cursor.rest()[..end]));
        cursor.advance(end + 1);
    }

    Ok(Link {
        arrow,
        dashed,
        label,
    })
}

fn find_terminator<'a>(
    rest: &str,
    terminators: &[(&'a str, bool)],
) -> Option<(usize, &'a str, bool)> {
    terminators
        .iter()
        .filter_map(|(text, arrow)| rest.find(text).map(|offset| (offset, *text, *arrow)))
        .min_by_key(|(offset, _, _)| *offset)
}

struct Cursor<'a> {
    text: &'a str,
    position: usize,
}

impl<'a> Cursor<'a> {
    fn new(text: &'a str) -> Self {
        Self { text, position: 0 }
    }

    fn rest(&self) -> &'a str {
        &self.text[self.position..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.position += ch.len_utf8();
        Some(ch)
    }

    fn advance(&mut self, bytes: usize) {
        self.position = (self.position + bytes).min(self.text.len());
    }

    fn advance_while(&mut self, expected: char) {
        while self.peek() == Some(expected) {
            self.bump();
        }
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(ch) if ch.is_whitespace()) {
            self.bump();
        }
    }

    fn eat(&mut self, expected: &str) -> bool {
        if self.rest().starts_with(expected) {
            self.advance(expected.len());
            true
        } else {
            false
        }
    }

    fn at_end(&self) -> bool {
        self.position >= self.text.len()
    }
}

#[cfg(test)]
mod tests {
    use super::{Direction, mermaid_elements, parse_flowchart};
    use crate::element::{BoundElementType, Element};

    const SAMPLE: &str = "\
flowchart TD
    A[Christmas] -->|Get money| B(Go shopping)
    B --> C{Let me think}
    C -->|One| D[Laptop]
    C -->|Two| E[iPhone]
    C -->|Three| F[Car]
";

    #[test]
    fn parses_nodes_and_edges() {
        let flowchart = parse_flowchart(SAMPLE).unwrap();
        assert_eq!(flowchart.direction, Some(Direction::TopDown));
        assert_eq!(flowchart.nodes.len(), 6);
        assert_eq!(flowchart.edges.len(), 5);
        assert_eq!(flowchart.edges[0].label.as_deref(), Some("Get money"));
        assert_eq!(flowchart.nodes[0].label, "Christmas");
    }

    #[test]
    fn generates_bound_text_and_arrows() {
        let elements = mermaid_elements(SAMPLE).unwrap();
        let containers = elements
            .iter()
            .filter(|element| {
                matches!(
                    element,
                    Element::Rectangle { .. } | Element::Diamond { .. } | Element::Ellipse { .. }
                )
            })
            .count();
        let bound_texts = elements
            .iter()
            .filter(|element| match element {
                Element::Text(text) => text.container_id.is_some(),
                _ => false,
            })
            .count();
        let arrows = elements
            .iter()
            .filter(|element| matches!(element, Element::Arrow(_)))
            .count();
        assert_eq!(containers, 6);
        assert_eq!(bound_texts, 10);
        assert_eq!(arrows, 5);

        for element in &elements {
            if let Element::Rectangle { base } = element
                && let Some(bound) = &base.bound_elements
            {
                assert_eq!(bound[0].kind, BoundElementType::Text);
            }
        }
        let labelled_arrows = elements
            .iter()
            .filter(|element| match element {
                Element::Arrow(a) => a.linear.base.bound_elements.is_some(),
                _ => false,
            })
            .count();
        assert_eq!(labelled_arrows, 4);
    }

    #[test]
    fn arrow_labels_sit_at_the_arrow_midpoint() {
        let elements = mermaid_elements(SAMPLE).unwrap();
        let mut checked = 0;
        for element in &elements {
            let Element::Arrow(arrow) = element else {
                continue;
            };
            let Some(bound) = &arrow.linear.base.bound_elements else {
                continue;
            };
            let label = elements
                .iter()
                .find(|candidate| candidate.id() == bound[0].id)
                .unwrap();
            let Element::Text(text) = label else {
                panic!("arrow label is not a text element");
            };
            let points = &arrow.linear.points;
            let mid = points[0].lerp(points[points.len() - 1], 0.5);
            let center = crate::geometry::Point::new(
                text.base.x + text.base.width / 2.0,
                text.base.y + text.base.height / 2.0,
            );
            assert!((center.x - mid.x).abs() < 0.5, "{center:?} vs {mid:?}");
            assert!((center.y - mid.y).abs() < 0.5, "{center:?} vs {mid:?}");
            checked += 1;
        }
        assert_eq!(checked, 4);
    }

    #[test]
    fn top_down_layout_increases_y() {
        let flowchart = parse_flowchart(SAMPLE).unwrap();
        let elements = mermaid_elements(SAMPLE).unwrap();
        let first = elements
            .iter()
            .find(|element| match element {
                Element::Rectangle { base } => base.width > 0.0 && base.y < 100.0,
                _ => false,
            })
            .unwrap();
        assert!(first.base().y < 100.0);
        assert!(!flowchart.edges.is_empty());
    }

    #[test]
    fn supports_left_right_and_other_shapes() {
        let source = "flowchart LR\n A([Start]) --> B[[Step]]\n B -.-> C((End))";
        let flowchart = parse_flowchart(source).unwrap();
        assert_eq!(flowchart.direction, Some(Direction::LeftRight));
        let elements = mermaid_elements(source).unwrap();
        assert!(
            elements
                .iter()
                .any(|element| matches!(element, Element::Ellipse { .. }))
        );
        let dashed = elements.iter().any(|element| match element {
            Element::Arrow(a) => a.linear.base.stroke_style == crate::types::StrokeStyle::Dashed,
            _ => false,
        });
        assert!(dashed);
    }

    #[test]
    fn rejects_unsupported_diagrams() {
        assert!(mermaid_elements("sequenceDiagram\n A->>B: hi").is_err());
        assert!(mermaid_elements("").is_err());
    }
}
