//! Turn only extension AST nodes into portable HTML, retaining ordinary
//! Markdown for the existing compiler (including references and footnotes).
use base64::Engine as _;
use markdown::mdast::Node;

pub fn extensions(source: &str, picture: bool) -> String {
    rewrite(source, picture, 0)
}
fn rewrite(source: &str, picture: bool, depth: usize) -> String {
    if depth > 24 {
        return source.into();
    }
    let mut options = markdown::ParseOptions::gfm();
    options.constructs.math_flow = true;
    options.constructs.math_text = true;
    let Ok(tree) = markdown::to_mdast(source, &options) else {
        return source.into();
    };
    let mut replacements = Vec::new();
    collect(&tree, source, picture, depth, &mut replacements);
    replacements.sort_by_key(|(r, _)| r.start);
    let mut out = String::with_capacity(source.len());
    let mut at = 0;
    for (range, html) in replacements {
        if range.start < at {
            continue;
        }
        out.push_str(&source[at..range.start]);
        out.push_str(&html);
        at = range.end;
    }
    out.push_str(&source[at..]);
    out
}
fn collect(
    node: &Node,
    source: &str,
    picture: bool,
    depth: usize,
    out: &mut Vec<(std::ops::Range<usize>, String)>,
) {
    if let Some(callout) = crate::callout::Callout::parse(node) {
        let Some(position) = node.position() else {
            return;
        };
        let range = position.start.offset..position.end.offset;
        let (body, _) = crate::callout::body_source(&source[range.clone()]);
        let body = rewrite(&body, picture, depth + 1);
        let (r, g, b) = callout.kind.tint();
        let color = callout.kind.accent(false);
        let kind = callout.kind.name();
        let html = format!(
            "\n\n<aside class=\"callout callout-{kind}\" style=\"background:var(--callout-bg,#{r:02x}{g:02x}{b:02x});border-left:3px solid var(--callout-accent,#{color:06x});border-radius:10px;padding:14px 16px;color:var(--text,#1f2328);margin:0 0 1em\">\n<div class=\"callout-title\" style=\"font-weight:600;color:var(--callout-accent,#{color:06x});margin-bottom:6px\">{}</div>\n\n{body}\n\n</aside>\n\n",
            super::html::escape(&callout.title)
        );
        out.push((range, html));
        return;
    }
    let math = match node {
        Node::Math(m) => Some((&m.value, true)),
        Node::InlineMath(m) => Some((&m.value, false)),
        _ => None,
    };
    if let Some((text, display)) = math {
        if let Some(position) = node.position() {
            out.push((
                position.start.offset..position.end.offset,
                equation(text, display, picture),
            ));
        }
        return;
    }
    if let Some(children) = node.children() {
        for child in children {
            collect(child, source, picture, depth, out);
        }
    }
}
fn equation(source: &str, display: bool, picture: bool) -> String {
    let escaped = super::html::escape(source);
    let Ok(formula) = crate::math::render(source, display, false) else {
        return if display {
            format!("\n\n<pre class=\"math-error\"><code>{escaped}</code></pre>\n\n")
        } else {
            format!("<code class=\"math-error\">{escaped}</code>")
        };
    };
    let image = if picture {
        match crate::math::png(&formula, 3.) {
            Some(png) => format!(
                "<img src=\"data:image/png;base64,{}\" width=\"{}\" height=\"{}\" alt=\"{escaped}\" style=\"vertical-align:{}px\" />",
                base64::engine::general_purpose::STANDARD.encode(png),
                formula.width,
                formula.height,
                formula.baseline - formula.height
            ),
            None => format!("<code>{escaped}</code>"),
        }
    } else {
        let svg = formula
            .svg
            .replace('\n', "")
            .replace("fill=\"#1f2328\"", "fill=\"currentColor\"");
        format!(
            "<span class=\"math-svg\" style=\"display:inline-block;width:{}em;height:{}em;vertical-align:{}em\">{svg}</span>",
            formula.width / 16.,
            formula.height / 16.,
            (formula.baseline - formula.height) / 16.
        )
    };
    if display {
        format!(
            "\n\n<div class=\"math-block\" role=\"math\" aria-label=\"{escaped}\" style=\"text-align:center;margin:1.2em 0\">{image}</div>\n\n"
        )
    } else {
        format!("<span class=\"math-inline\" role=\"math\" aria-label=\"{escaped}\">{image}</span>")
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn html_exports_math_as_svg_and_callouts_as_semantic_tinted_panels() {
        let source = "> [!NOTE]\n> A **note** with $x^2$.\n\n> [!TIP] Helpful\n> - A list\n> - Another item\n\n> [!WARNING]\n> Be careful.\n\n$$\n\\frac{1}{2}+\\sqrt{x^2+y^2}\n$$\n\n```md\n> [!NOTE]\n$$ literal $$\n```\n";
        let html = super::super::html::body(source, None, &mut super::super::html::Images::None);
        assert!(html.contains("callout-note"), "{html}");
        assert!(html.contains("callout-tip"));
        assert!(html.contains("callout-warning"));
        assert!(html.contains("<strong>note</strong>"));
        assert!(html.contains("<li>A list</li>"));
        assert!(html.contains("role=\"math\""));
        assert!(html.contains("<svg"));
        assert!(html.contains("<path"));
        assert!(!html.contains("language-math"));
        assert!(html.contains("$$ literal $$"));
        let mut inline = super::super::html::Images::Inline { used: 0 };
        let clipboard = super::super::html::body(source, None, &mut inline);
        assert!(clipboard.contains("data:image/png;base64,"));
        assert!(clipboard.contains("callout-warning"));
        std::fs::write("/tmp/enso-callout-math.html", html).unwrap();
    }
}
