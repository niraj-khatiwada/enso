use gpui_excalidraw::ui::i18n::{I18n, Language};

fn source_files() -> Vec<(String, String)> {
    fn walk(dir: &std::path::Path, out: &mut Vec<(String, String)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                out.push((name, std::fs::read_to_string(&path).unwrap()));
            }
        }
    }
    let mut out = Vec::new();
    walk(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .as_path(),
        &mut out,
    );
    out
}

fn referenced_keys(src: &str) -> Vec<String> {
    let mut keys = Vec::new();
    for (idx, _) in src.match_indices("t(\"") {
        if src
            .as_bytes()
            .get(idx.wrapping_sub(1))
            .is_some_and(|prev| prev.is_ascii_alphanumeric() || *prev == b'_')
        {
            continue;
        }
        let rest = &src[idx + 3..];
        if let Some(end) = rest.find('"') {
            let key = &rest[..end];
            if !key.is_empty() {
                keys.push(key.to_string());
            }
        }
    }
    keys
}

#[test]
fn every_key_the_ui_requests_is_translated_in_every_language() {
    let mut missing = Vec::new();
    for (file, src) in source_files() {
        for key in referenced_keys(&src) {
            for language in Language::all() {
                let i18n = I18n::new(language);
                if i18n.t(&key) == key {
                    missing.push(format!(
                        "{file}: t(\"{key}\") is untranslated in {language:?}"
                    ));
                }
            }
        }
    }
    missing.sort();
    missing.dedup();
    assert!(
        missing.is_empty(),
        "raw translation keys would reach the user:\n{}",
        missing.join("\n")
    );
}

#[test]
fn english_is_a_superset_of_every_other_locale() {
    let all = gpui_excalidraw::ui::i18n::translations();
    let en = all.get("en").expect("an English table");

    let mut orphans = Vec::new();
    for (code, table) in &all {
        if *code == "en" {
            continue;
        }
        for (key, value) in table.iter() {
            if !en.contains_key(key) {
                orphans.push(format!("{code}: \"{key}\" = {value:?}"));
            }
        }
    }
    orphans.sort();
    assert!(
        orphans.is_empty(),
        "these translations can never be displayed because English has no \
         matching key:\n{}",
        orphans.join("\n")
    );
}

#[test]
fn all_listeners_go_through_the_notify_funnel() {
    let mut raw = Vec::new();
    for (file, src) in source_files() {
        for (line_no, line) in src.lines().enumerate() {
            if line.contains("cx.listener(") {
                raw.push(format!("{file}:{}: {}", line_no + 1, line.trim()));
            }
        }
    }
    assert_eq!(
        raw.len(),
        1,
        "only `Editor::act` may use Context::listener directly, so that every \
         handler notifies. Found:\n{}",
        raw.join("\n")
    );
    assert!(
        raw[0].starts_with("editor.rs"),
        "the single allowed use should be inside Editor::act, found {}",
        raw[0]
    );
}

#[test]
fn every_kbd_marker_is_balanced_in_every_locale() {
    let mut broken = Vec::new();
    for (code, table) in gpui_excalidraw::ui::i18n::translations() {
        for (key, value) in table.iter() {
            let opens = value.matches("{{").count();
            let closes = value.matches("}}").count();
            if opens != closes {
                broken.push(format!(
                    "{code}: \"{key}\" has {opens} `{{{{` but {closes} `}}}}`"
                ));
            }
            if value.contains("{{}}") {
                broken.push(format!("{code}: \"{key}\" has an empty `{{{{}}}}` chip"));
            }
        }
    }
    broken.sort();
    assert!(
        broken.is_empty(),
        "unbalanced kbd markers would eat hint text:\n{}",
        broken.join("\n")
    );
}

#[test]
fn the_canvas_hint_still_marks_its_key_names() {
    let parts = gpui_excalidraw::ui::split_hint(&I18n::new(Language::En).t("hint.moveCanvas"));
    let keys: Vec<&str> = parts
        .iter()
        .filter(|(_, is_key)| *is_key)
        .map(|(run, _)| run.as_str())
        .collect();
    assert_eq!(
        keys,
        vec!["Scroll wheel", "Space"],
        "the hint should chip exactly the two key names upstream chips"
    );
    let rendered: String = parts.iter().map(|(run, _)| run.as_str()).collect();
    assert_eq!(
        rendered, "To move canvas, hold Scroll wheel or Space while dragging, or use the hand tool",
        "stripping the markers must reproduce upstream's plain sentence"
    );
}

#[test]
fn table_driven_labels_are_explicit_in_every_locale() {
    let all = gpui_excalidraw::ui::i18n::translations();

    let mut keys: Vec<&'static str> = gpui_excalidraw::ui::Editor::palette_labels();
    keys.extend(gpui_excalidraw::ui::Editor::help_labels());
    keys.sort();
    keys.dedup();

    let mut missing = Vec::new();
    for key in keys {
        for (code, table) in &all {
            if !table.contains_key(key) {
                missing.push(format!("{code}: \"{key}\""));
            }
        }
    }
    missing.sort();
    missing.dedup();
    assert!(
        missing.is_empty(),
        "these palette/help labels would show English or a raw key:\n{}",
        missing.join("\n")
    );
}

#[test]
fn ui_referenced_keys_are_explicit_in_every_locale() {
    let all = gpui_excalidraw::ui::i18n::translations();
    let en = all.get("en").expect("an English table");

    let mut missing = Vec::new();
    for (file, src) in source_files() {
        for key in referenced_keys(&src) {
            if !en.contains_key(key.as_str()) {
                continue;
            }
            for (code, table) in &all {
                if *code == "en" || table.contains_key(key.as_str()) {
                    continue;
                }
                missing.push(format!("{code}: \"{key}\" (used in {file})"));
            }
        }
    }
    missing.sort();
    missing.dedup();
    assert!(
        missing.is_empty(),
        "these locales would silently show English text:\n{}",
        missing.join("\n")
    );
}
