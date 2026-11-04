//! Local OS dictionaries, debounced off the UI thread. Decorations belong to
//! this controller so they never replace syntax colors or LSP diagnostics.
use gpui_kit::component::{
    ActiveTheme,
    input::{EditorState, InputEvent, TextDecoration, TextDecorationCollection},
    native_menu::NativeMenu,
};
use gpui_kit::*;
use std::{ops::Range, time::Duration};

const MAX_FILE_BYTES: usize = 5 * 1024 * 1024 / 2;
const MAX_ISSUES: usize = 100;

#[derive(Clone, Debug)]
struct Issue {
    range: Range<usize>,
    word: String,
    suggestions: Vec<String>,
}

#[derive(Action, Clone, PartialEq)]
#[action(namespace = spelling, no_json)]
pub struct ReplaceSpelling {
    editor: EntityId,
    range: Range<usize>,
    word: String,
    replacement: String,
}

pub struct Spellcheck {
    editor: WeakEntity<EditorState>,
    snapshot: gpui_kit::component::input::Rope,
    decorations: TextDecorationCollection,
    issues: Vec<Issue>,
    enabled: bool,
    too_large: bool,
    task: Option<Task<()>>,
    generation: u64,
    pub error: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl Spellcheck {
    pub fn new(editor: &Entity<EditorState>, source_len: usize, cx: &mut Context<Self>) -> Self {
        let decorations = editor.update(cx, |editor, cx| {
            editor.create_decorations_collection(Vec::new(), cx)
        });
        let changes = cx.subscribe(editor, |this, _, event, cx| {
            if matches!(event, InputEvent::Change | InputEvent::Focus) {
                this.schedule(cx);
            }
        });
        // Programmatic reloads and initial large-editor loading intentionally
        // emit no Change event. Rope identity detects those too, in O(1),
        // without rechecking after cursor moves or decoration-only updates.
        let reloads = cx.observe(editor, |this, editor, cx| {
            if !ropey::extra::esoterica::ropes_are_instances(&this.snapshot, editor.read(cx).text())
            {
                this.schedule(cx);
            }
        });
        let preferences = cx.observe_global::<crate::settings::Preferences>(|this, cx| {
            if this.enabled != crate::settings::spell_check(cx) {
                this.schedule(cx);
            }
        });
        let mut this = Self {
            editor: editor.downgrade(),
            snapshot: editor.read(cx).text().clone(),
            decorations,
            issues: Vec::new(),
            enabled: crate::settings::spell_check(cx),
            too_large: source_len >= MAX_FILE_BYTES,
            task: None,
            generation: 0,
            error: None,
            _subscriptions: vec![changes, reloads, preferences],
        };
        this.schedule(cx);
        this
    }

    fn schedule(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.upgrade() else {
            return;
        };
        let text = editor.read(cx).text().clone();
        if text.len() > 0 || self.snapshot.len() > 0 {
            self.too_large = text.len() >= MAX_FILE_BYTES;
        }
        self.snapshot = text.clone();
        self.enabled = crate::settings::spell_check(cx);
        self.generation += 1;
        self.task = None;
        self.issues.clear();
        self.decorations.clear(cx);
        self.error = None;
        if !self.enabled {
            return;
        }
        if text.len() == 0 {
            return;
        }
        if self.too_large {
            return;
        }
        let generation = self.generation;
        self.task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(600))
                .await;
            let result = cx
                .background_executor()
                .spawn(async move {
                    let source = text.to_string();
                    let masked = prose_only(&source);
                    platform::check(&masked)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if generation != this.generation || !crate::settings::spell_check(cx) {
                    return;
                }
                match result {
                    Ok(issues) => this.present(issues, cx),
                    Err(error) => {
                        this.error = Some(error);
                        cx.notify();
                    }
                }
            });
        }));
    }

    fn present(&mut self, issues: Vec<Issue>, cx: &mut Context<Self>) {
        let color = cx.theme().danger;
        self.decorations.set(
            issues
                .iter()
                .map(|issue| {
                    TextDecoration::new(
                        issue.range.clone(),
                        HighlightStyle {
                            underline: Some(UnderlineStyle {
                                thickness: px(1.),
                                color: Some(color),
                                wavy: true,
                            }),
                            ..Default::default()
                        },
                    )
                })
                .collect(),
            cx,
        );
        self.issues = issues;
        cx.notify();
    }
}

/// Keep UTF-8 byte positions intact while masking code, destinations, tags,
/// metadata and Markdown punctuation. Native UTF-16 results are mapped back
/// through this masked string, not the original document.
fn prose_only(source: &str) -> String {
    fn visit(node: &markdown::mdast::Node, source: &str, output: &mut [u8], metadata_end: usize) {
        if let markdown::mdast::Node::Text(_) = node {
            if let Some(position) = node.position() {
                let range = position.start.offset..position.end.offset;
                if range.start >= metadata_end && range.end <= source.len() {
                    output[range.clone()].copy_from_slice(&source.as_bytes()[range]);
                }
            }
        } else if !matches!(
            node,
            markdown::mdast::Node::Code(_)
                | markdown::mdast::Node::InlineCode(_)
                | markdown::mdast::Node::Html(_)
                | markdown::mdast::Node::Image(_)
                | markdown::mdast::Node::ImageReference(_)
                | markdown::mdast::Node::Definition(_)
        ) {
            if let Some(children) = node.children() {
                for child in children {
                    visit(child, source, output, metadata_end);
                }
            }
        }
    }
    let mut output = source
        .bytes()
        .map(|b| if b == b'\n' { b'\n' } else { b' ' })
        .collect::<Vec<_>>();
    let metadata_end = if source.starts_with("---\n") || source.starts_with("---\r\n") {
        let mut offset = source.lines().next().map_or(0, |s| s.len() + 1);
        source
            .split_inclusive('\n')
            .skip(1)
            .find_map(|line| {
                offset += line.len();
                matches!(line.trim(), "---" | "...").then_some(offset)
            })
            .unwrap_or(0)
    } else {
        0
    };
    if let Ok(root) = markdown::to_mdast(source, &markdown::ParseOptions::gfm()) {
        visit(&root, source, &mut output, metadata_end);
    }
    String::from_utf8(output).expect("text spans preserve UTF-8 boundaries")
}

fn utf16_range(source: &str, start: usize, length: usize) -> Option<Range<usize>> {
    let mut units = 0;
    let mut first = None;
    for (byte, ch) in source
        .char_indices()
        .chain(std::iter::once((source.len(), '\0')))
    {
        if units == start {
            first = Some(byte);
        }
        if units == start.checked_add(length)? {
            return Some(first?..byte);
        }
        units += ch.len_utf16();
    }
    None
}

pub fn context_menu(
    mut menu: NativeMenu,
    editor: &Entity<EditorState>,
    checker: Option<&Entity<Spellcheck>>,
    cx: &App,
) -> NativeMenu {
    if crate::settings::spell_check(cx) {
        if checker.is_some_and(|checker| checker.read(cx).too_large) {
            menu = menu
                .menu_with_disabled(
                    "Spell Check Disabled for Large File",
                    true,
                    Box::new(gpui_kit::component::input::SelectAll),
                )
                .separator();
        }
        let cursor = editor.read(cx).cursor();
        if let Some(issue) =
            checker.and_then(|c| c.read(cx).issues.iter().find(|i| i.range.contains(&cursor)))
        {
            for suggestion in &issue.suggestions {
                menu = menu.menu(
                    suggestion.clone(),
                    Box::new(ReplaceSpelling {
                        editor: editor.entity_id(),
                        range: issue.range.clone(),
                        word: issue.word.clone(),
                        replacement: suggestion.clone(),
                    }),
                );
            }
            if !issue.suggestions.is_empty() {
                menu = menu.separator();
            }
        }
    }
    let selected = !editor.read(cx).selected_range().is_empty();
    menu.menu_with_disabled("Cut", !selected, Box::new(gpui_kit::component::input::Cut))
        .menu_with_disabled(
            "Copy",
            !selected,
            Box::new(gpui_kit::component::input::Copy),
        )
        .menu("Paste", Box::new(gpui_kit::component::input::Paste))
        .separator()
        .menu(
            "Select all",
            Box::new(gpui_kit::component::input::SelectAll),
        )
}

impl crate::Enso {
    pub fn replace_spelling(
        &mut self,
        action: &ReplaceSpelling,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(doc) = self
            .documents
            .iter()
            .find(|d| d.editor.entity_id() == action.editor)
        else {
            return;
        };
        doc.editor.update(cx, |editor, cx| {
            if editor
                .text()
                .try_slice(action.range.clone())
                .ok()
                .is_some_and(|s| s.to_string() == action.word)
            {
                editor.set_selected_range(action.range.clone(), cx);
                editor.replace(&action.replacement, window, cx);
            }
        });
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use objc2_app_kit::NSSpellChecker;
    use objc2_foundation::NSString;
    pub(super) fn check(source: &str) -> Result<Vec<Issue>, String> {
        // The shared checker is serialized across worker jobs. No AppKit UI or
        // document/editor state is accessed on this thread.
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = LOCK.lock().map_err(|e| e.to_string())?;
        objc2::rc::autoreleasepool(|_| {
            let checker = NSSpellChecker::sharedSpellChecker();
            let string = NSString::from_str(source);
            let tag = NSSpellChecker::uniqueSpellDocumentTag();
            let mut issues = Vec::new();
            let mut offset = 0;
            while issues.len() < MAX_ISSUES {
                let found = unsafe {
                    checker.checkSpellingOfString_startingAt_language_wrap_inSpellDocumentWithTag_wordCount(
                    &string, offset as isize, None, false, tag, std::ptr::null_mut())
                };
                if found.length == 0 {
                    break;
                }
                let Some(range) = utf16_range(source, found.location, found.length) else {
                    break;
                };
                let suggestions = checker
                    .guessesForWordRange_inString_language_inSpellDocumentWithTag(
                        found, &string, None, tag,
                    )
                    .map(|list| list.iter().take(5).map(|s| s.to_string()).collect())
                    .unwrap_or_default();
                issues.push(Issue {
                    word: source[range.clone()].to_owned(),
                    range,
                    suggestions,
                });
                let next = found.location + found.length;
                if next <= offset {
                    break;
                }
                offset = next;
            }
            checker.closeSpellDocumentWithTag(tag);
            Ok(issues)
        })
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use windows::{
        Win32::{Globalization::*, System::Com::*},
        core::{HSTRING, PCWSTR, PWSTR},
    };
    pub(super) fn check(source: &str) -> Result<Vec<Issue>, String> {
        unsafe fn run(source: &str) -> windows::core::Result<Vec<Issue>> {
            unsafe {
                let factory: ISpellCheckerFactory =
                    CoCreateInstance(&SpellCheckerFactory, None, CLSCTX_INPROC_SERVER)?;
                let mut locale = [0_u16; 85];
                let count = GetUserDefaultLocaleName(&mut locale);
                let language = if count > 1 {
                    HSTRING::from_wide(&locale[..count as usize - 1])
                } else {
                    HSTRING::from("en-US")
                };
                let checker = factory
                    .CreateSpellChecker(&language)
                    .or_else(|_| factory.CreateSpellChecker(windows::core::w!("en-US")))?;
                let errors = checker.Check(&HSTRING::from(source))?;
                let mut issues = Vec::new();
                while issues.len() < MAX_ISSUES {
                    let mut error = None;
                    errors.Next(&mut error).ok()?;
                    let Some(error) = error else {
                        break;
                    };
                    let Some(range) = utf16_range(
                        source,
                        error.StartIndex()? as usize,
                        error.Length()? as usize,
                    ) else {
                        break;
                    };
                    let word = source[range.clone()].to_owned();
                    let mut suggestions = Vec::new();
                    if let Ok(list) = checker.Suggest(&HSTRING::from(&word)) {
                        for _ in 0..5 {
                            let mut string = [PWSTR::null()];
                            let mut fetched = 0;
                            if list.Next(&mut string, Some(&mut fetched)).is_err() || fetched == 0 {
                                break;
                            }
                            if !string[0].is_null() {
                                if let Ok(value) = PCWSTR(string[0].0).to_string() {
                                    suggestions.push(value);
                                }
                                CoTaskMemFree(Some(string[0].0.cast()));
                            }
                        }
                    }
                    issues.push(Issue {
                        range,
                        word,
                        suggestions,
                    });
                }
                Ok(issues)
            }
        }
        unsafe {
            let init = CoInitializeEx(None, COINIT_MULTITHREADED);
            if init.is_err() {
                return Err(format!("Could not initialize spelling service: {init:?}"));
            }
            let result = run(source).map_err(|e| e.to_string());
            CoUninitialize();
            result
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use std::ffi::{CStr, CString, c_char, c_int, c_void};
    pub(super) fn check(source: &str) -> Result<Vec<Issue>, String> {
        // Enchant is optional at runtime, so installing enso never introduces
        // a hard linker dependency or prevents launch on a minimal desktop.
        unsafe {
            let library = libloading::Library::new("libenchant-2.so.2")
                .or_else(|_| libloading::Library::new("libenchant-2.so"))
                .map_err(|_| {
                    "Install Enchant 2 and a spelling dictionary to enable spell check.".to_string()
                })?;
            macro_rules! symbol {
                ($name:literal, $ty:ty) => {
                    *library
                        .get::<$ty>(concat!($name, "\0").as_bytes())
                        .map_err(|e| e.to_string())?
                };
            }
            let init = symbol!("enchant_broker_init", unsafe extern "C" fn() -> *mut c_void);
            let free = symbol!("enchant_broker_free", unsafe extern "C" fn(*mut c_void));
            let request = symbol!(
                "enchant_broker_request_dict",
                unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void
            );
            let free_dict = symbol!(
                "enchant_broker_free_dict",
                unsafe extern "C" fn(*mut c_void, *mut c_void)
            );
            let check = symbol!(
                "enchant_dict_check",
                unsafe extern "C" fn(*mut c_void, *const c_char, isize) -> c_int
            );
            let suggest = symbol!(
                "enchant_dict_suggest",
                unsafe extern "C" fn(
                    *mut c_void,
                    *const c_char,
                    isize,
                    *mut usize,
                ) -> *mut *mut c_char
            );
            let free_strings = symbol!(
                "enchant_dict_free_string_list",
                unsafe extern "C" fn(*mut c_void, *mut *mut c_char)
            );
            let broker = init();
            if broker.is_null() {
                return Err("Could not start Enchant.".into());
            }
            let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
                .into_iter()
                .find_map(|key| std::env::var(key).ok().filter(|v| !v.is_empty()))
                .unwrap_or_else(|| "en_US".into());
            let locale = locale
                .split('.')
                .next()
                .unwrap_or("en_US")
                .split('@')
                .next()
                .unwrap_or("en_US")
                .replace('-', "_");
            let mut dict = std::ptr::null_mut();
            for language in [locale.as_str(), "en_US", "en_GB"] {
                if let Ok(language) = CString::new(language) {
                    dict = request(broker, language.as_ptr());
                }
                if !dict.is_null() {
                    break;
                }
            }
            if dict.is_null() {
                free(broker);
                return Err("Install a Hunspell dictionary for your language.".into());
            }
            let mut issues = Vec::new();
            let mut start = None;
            for (i, ch) in source
                .char_indices()
                .chain(std::iter::once((source.len(), ' ')))
            {
                if ch.is_alphabetic() || ((ch == '\'' || ch == '’') && start.is_some()) {
                    start.get_or_insert(i);
                    continue;
                }
                if let Some(start) = start.take() {
                    let word = source[start..i].trim_end_matches(['\'', '’']);
                    if word.chars().count() < 2 || word.len() > 128 {
                        continue;
                    }
                    if check(dict, word.as_ptr().cast(), word.len() as isize) != 1 {
                        continue;
                    }
                    let mut count = 0;
                    let list = suggest(dict, word.as_ptr().cast(), word.len() as isize, &mut count);
                    let mut suggestions = Vec::new();
                    if !list.is_null() {
                        for i in 0..count.min(5) {
                            suggestions
                                .push(CStr::from_ptr(*list.add(i)).to_string_lossy().into_owned());
                        }
                        free_strings(dict, list);
                    }
                    issues.push(Issue {
                        range: start..start + word.len(),
                        word: word.into(),
                        suggestions,
                    });
                    if issues.len() >= MAX_ISSUES {
                        break;
                    }
                }
            }
            free_dict(broker, dict);
            free(broker);
            Ok(issues)
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
mod platform {
    use super::*;
    pub(super) fn check(_: &str) -> Result<Vec<Issue>, String> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn checks_prose_without_code_urls_metadata_or_html_tags() {
        let source = "---\ntitle: metadataword\n---\n\n# Welcomme\n\n[linklabel](https://some.invalid/typpo) `inlinecodeword`\n\n```rust\ncodeword\n```\n\n<mark color=\"#ff0000\">highligt</mark>\n\n- [ ] checkboxtypo\n\nनमस्ते 🟠";
        let masked = prose_only(source);
        assert_eq!(masked.len(), source.len());
        for word in ["Welcomme", "linklabel", "highligt", "checkboxtypo", "नमस्ते"] {
            assert!(masked.contains(word), "{word}: {masked}");
        }
        for word in [
            "metadataword",
            "https",
            "typpo",
            "inlinecodeword",
            "codeword",
            "color",
        ] {
            assert!(!masked.contains(word), "{word}");
        }
    }
    #[::core::prelude::v1::test]
    fn utf16_offsets_map_back_to_utf8() {
        assert_eq!(utf16_range("a🟠न typo", 5, 4), Some(9..13));
        assert_eq!(utf16_range("a🟠न typo", 2, 1), None);
    }

    #[cfg(target_os = "macos")]
    #[::core::prelude::v1::test]
    #[ignore = "requires the macOS spelling service and dictionaries"]
    fn native_spelling_service_returns_suggestions() {
        let issues = platform::check("This sentnce has a mispelled word.").unwrap();
        assert!(
            !issues.is_empty(),
            "native spelling service must flag a typo"
        );
        assert!(
            issues.iter().any(|i| !i.suggestions.is_empty()),
            "native suggestions must be available"
        );
    }
}

#[cfg(all(test, feature = "ui-tests"))]
mod ui_tests {
    use super::{Issue, ReplaceSpelling};
    use crate::{Enso, Mode, settings};
    use ::core::prelude::v1::test;
    use gpui_kit::{AppContext, BorrowAppContext, TestAppContext, WindowOptions};

    #[gpui_kit::test]
    fn large_files_skip_checking_without_changing_the_global_preference(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            settings::init(cx);
            cx.update_global::<settings::Preferences, _>(|p, _| p.spell_check = true);
        });
        let (handle, app) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    let mut app = Enso::new(window, cx);
                    app.add_document(None, "x".repeat(super::MAX_FILE_BYTES), None, window, cx);
                    app
                })
            })
            .unwrap()
        });
        let (editor, checker) = cx.update(|cx| {
            let doc = &app.read(cx).documents[0];
            (doc.editor.clone(), doc.spellcheck.clone().unwrap())
        });
        cx.update(|cx| {
            assert!(settings::spell_check(cx));
            assert!(checker.read(cx).too_large);
            assert!(checker.read(cx).task.is_none());
        });
        cx.update_window(handle, |_, w, cx| {
            app.update(cx, |app, cx| app.set_mode(Mode::Edit, w, cx))
        })
        .unwrap();
        cx.run_until_parked();
        cx.update(|cx| {
            assert!(checker.read(cx).too_large);
            assert!(checker.read(cx).task.is_none());
        });
        // Shrinking below the limit resumes checks for this file.
        cx.update_window(handle, |_, w, cx| {
            editor.update(cx, |editor, cx| editor.set_value("A short note.", w, cx))
        })
        .unwrap();
        cx.run_until_parked();
        cx.update(|cx| {
            assert!(!checker.read(cx).too_large);
            assert!(checker.read(cx).task.is_some());
            assert!(settings::spell_check(cx));
        });
        // Growing back to exactly the threshold disables it again.
        cx.update_window(handle, |_, w, cx| {
            editor.update(cx, |editor, cx| {
                editor.set_value("x".repeat(super::MAX_FILE_BYTES), w, cx)
            })
        })
        .unwrap();
        cx.run_until_parked();
        cx.update(|cx| {
            assert!(checker.read(cx).too_large);
            assert!(checker.read(cx).task.is_none());
            assert!(settings::spell_check(cx));
        });
    }

    #[gpui_kit::test]
    fn disabling_clears_underlines_and_correction_is_undoable(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::bind_view_keys(cx);
            settings::init(cx);
        });
        let source = "# Héllo\n\nA mispelled word.";
        let range = source.find("mispelled").unwrap()..source.find("mispelled").unwrap() + 9;
        let (handle, app) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    let mut app = Enso::new(window, cx);
                    app.add_document(None, source.into(), None, window, cx);
                    app.set_mode(Mode::Edit, window, cx);
                    app
                })
            })
            .unwrap()
        });
        cx.update_window(handle, |_, w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
        })
        .unwrap();
        cx.run_until_parked();
        let checker = cx.update(|cx| app.read(cx).documents[0].spellcheck.clone().unwrap());
        cx.update(|cx| {
            checker.update(cx, |check, cx| {
                check.task = None;
                check.present(
                    vec![Issue {
                        range: range.clone(),
                        word: "mispelled".into(),
                        suggestions: vec!["misspelled".into()],
                    }],
                    cx,
                );
                assert_eq!(check.decorations.get_ranges(cx), vec![range.clone()]);
            })
        });
        cx.update(|cx| cx.update_global::<settings::Preferences, _>(|p, _| p.spell_check = false));
        cx.run_until_parked();
        cx.update(|cx| {
            assert!(checker.read(cx).decorations.get_ranges(cx).is_empty());
            assert!(checker.read(cx).issues.is_empty());
        });
        let editor = cx.update(|cx| app.read(cx).documents[0].editor.clone());
        cx.update_window(handle, |_, w, cx| {
            app.update(cx, |app, cx| {
                app.replace_spelling(
                    &ReplaceSpelling {
                        editor: editor.entity_id(),
                        range: range.clone(),
                        word: "mispelled".into(),
                        replacement: "misspelled".into(),
                    },
                    w,
                    cx,
                )
            })
        })
        .unwrap();
        assert_eq!(
            cx.update(|cx| editor.read(cx).value().to_string()),
            source.replace("mispelled", "misspelled")
        );
        cx.simulate_keystrokes(
            handle,
            if cfg!(target_os = "macos") {
                "cmd-z"
            } else {
                "ctrl-z"
            },
        );
        cx.run_until_parked();
        assert_eq!(cx.update(|cx| editor.read(cx).value().to_string()), source);
        // A stale suggestion must not replace unrelated, newly edited text.
        cx.update_window(handle, |_, w, cx| {
            app.update(cx, |app, cx| {
                app.replace_spelling(
                    &ReplaceSpelling {
                        editor: editor.entity_id(),
                        range,
                        word: "different".into(),
                        replacement: "wrong".into(),
                    },
                    w,
                    cx,
                )
            })
        })
        .unwrap();
        assert_eq!(cx.update(|cx| editor.read(cx).value().to_string()), source);
    }
}
