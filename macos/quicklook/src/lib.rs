//! Markdown to HTML for Enso's Quick Look extension, called from Swift.

use std::ffi::{CStr, CString, c_char};

/// Render GitHub-flavoured Markdown to an HTML fragment.
///
/// Raw HTML is passed through, so the `<img>` tags Enso inserts for pasted
/// images render; Quick Look runs previews without JavaScript, in a sandbox.
/// Front matter is parsed and left out of the output.
pub fn render(source: &str) -> String {
    let mut options = markdown::Options::gfm();
    options.parse.constructs.frontmatter = true;
    options.compile.allow_dangerous_html = true;
    markdown::to_html_with_options(source, &options).unwrap_or_else(|_| {
        let escaped = source
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        format!("<pre>{escaped}</pre>")
    })
}

/// Render the NUL-terminated UTF-8 Markdown in `source` to HTML. Free the
/// result with [`enso_quicklook_free`].
///
/// # Safety
/// `source` must be null or point to a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn enso_quicklook_render(source: *const c_char) -> *mut c_char {
    let source = if source.is_null() {
        Default::default()
    } else {
        // SAFETY: the caller passes a NUL-terminated string.
        unsafe { CStr::from_ptr(source) }.to_string_lossy()
    };
    CString::new(render(&source).replace('\0', ""))
        .unwrap_or_default()
        .into_raw()
}

/// Free HTML returned by [`enso_quicklook_render`].
///
/// # Safety
/// `html` must be null or a pointer returned by `enso_quicklook_render`, freed
/// only once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn enso_quicklook_free(html: *mut c_char) {
    if !html.is_null() {
        // SAFETY: the pointer came from `CString::into_raw` above.
        drop(unsafe { CString::from_raw(html) });
    }
}

#[cfg(test)]
mod tests {
    use super::render;

    #[test]
    fn renders_gfm_html_and_skips_front_matter() {
        let html = render(
            "---\ntitle: x\n---\n# Hi\n\n- [x] done\n\n| a |\n|---|\n| b |\n\n<img src=\"a.png\" width=\"100%\" />\n",
        );
        assert!(html.contains("<h1>Hi</h1>"));
        assert!(html.contains("type=\"checkbox\""));
        assert!(html.contains("<table>"));
        assert!(html.contains("<img src=\"a.png\" width=\"100%\" />"));
        assert!(!html.contains("title: x"));
    }
}
