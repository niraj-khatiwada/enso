use gpui_kit::component::input::EditorState;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Entity, EntityInputHandler as _, TestAppContext,
    WindowBounds, WindowOptions, point, px, size,
};
use unicode_segmentation::UnicodeSegmentation as _;

use super::view::VisualView;

const DOC: &str = "# Heading with **bold** words\n\nA paragraph with *emphasis*, `code`, a [link](u) and \\* escape &amp; more.\nSoft break नमस्ते दुनिया। 中文 👍🏽 end.\n\n- first item\n- [ ] second **task**\n  - nested\n\n> quoted\n> lines\n\n```rust\nfn main() {}\nlet 中 = 1;\n```\n\n| a | b |\n|---|--:|\n| 中文 | 👍🏽 x |\n";

fn open(cx: &mut TestAppContext, text: &str) -> (AnyWindowHandle, Entity<VisualView>) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        super::view::init(cx);
    });
    let text = text.to_string();
    let (handle, view) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: Default::default(),
                    size: size(px(1000.), px(2000.)),
                })),
                ..Default::default()
            },
            cx,
            |window, cx| {
                let editor = cx.new(|cx| EditorState::new(window, cx).default_value(text));
                let view = cx.new(|cx| VisualView::new(editor, window, cx));
                view.update(cx, |v, cx| window.focus(&v.focus, cx));
                view
            },
        )
        .unwrap()
    });
    draw(cx, handle);
    (handle, view)
}

fn draw(cx: &mut TestAppContext, handle: AnyWindowHandle) {
    cx.update_window(handle, |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    cx.run_until_parked();
}

fn source(cx: &mut TestAppContext, view: &Entity<VisualView>) -> String {
    cx.update(|cx| view.read(cx).editor.read(cx).text().to_string())
}

#[gpui_kit::test]
fn clicks_resolve_to_source_bytes(cx: &mut TestAppContext) {
    let (_, view) = open(cx, DOC);
    cx.update(|cx| {
        let view = view.read(cx);
        let geometry = view.geometry.borrow();
        let mut checked = 0;
        let mut kinds = std::collections::HashSet::new();
        for (&key, leaf) in geometry.iter() {
            let (bix, lix) = view.doc.leaf_by_key(key).unwrap();
            let block = &view.doc.blocks[bix];
            kinds.insert(std::mem::discriminant(&block.node));
            let map = &block.leaves[lix];
            assert_eq!(leaf.len(), map.len(), "leaf {:?}", map.text);
            let boundaries = map
                .text
                .grapheme_indices(true)
                .map(|(i, _)| i)
                .chain([map.len()]);
            for i in boundaries {
                let at = leaf.position_for_index(i);
                let click = point(at.x, at.y + leaf.line_height() / 2.);
                let src = view.source_for_point(click).unwrap();
                let expected = view.doc.src_of(bix, lix, i);
                let (b2, l2, i2) = view.doc.locate(src).unwrap();
                assert_eq!((b2, l2), (bix, lix), "{:?} at {i}", map.text);
                assert_eq!(
                    leaf.position_for_index(i2),
                    at,
                    "{:?} at {i}: clicked {src}, expected {expected}",
                    map.text
                );
                if let Some(c) = map.text[i..].chars().next()
                    && let Some(seg) = map.segs.iter().find(|s| s.linear && s.leaf.contains(&i))
                {
                    let at = block.start + seg.src.start + (i - seg.leaf.start);
                    assert!(DOC[at..].starts_with(c), "{:?} at {i}", map.text);
                }
                let _ = expected;
                checked += 1;
            }
        }
        assert!(kinds.len() >= 6, "{} block kinds", kinds.len());
        assert!(checked > 150, "{checked}");
    });
}

#[gpui_kit::test]
fn typing_updates_the_rope_and_reparses_one_block(cx: &mut TestAppContext) {
    let (handle, view) = open(cx, DOC);
    for anchor in ["दुनिया", "文 👍🏽", " end."] {
        let before = source(cx, &view);
        let at = before.find(anchor).unwrap();
        let blocks = cx.update(|cx| view.read(cx).doc.blocks.len());
        cx.update(|cx| view.update(cx, |v, _| v.caret = at));
        cx.simulate_input(handle, "x");
        draw(cx, handle);
        let after = source(cx, &view);
        let mut expected = before.clone();
        expected.insert(at, 'x');
        assert_eq!(after, expected);
        cx.update(|cx| {
            let v = view.read(cx);
            assert_eq!(v.caret, at + 1);
            assert_eq!(v.doc.blocks.len(), blocks);
            let parsed = v.last_edit.as_ref().unwrap().parsed_bytes;
            assert!(parsed < 400, "reparsed {parsed} bytes");
            assert!(v.caret_position().is_some());
        });
    }
}

#[gpui_kit::test]
fn ime_composition_marks_and_commits(cx: &mut TestAppContext) {
    let (handle, view) = open(cx, DOC);
    let at = DOC.find("end.").unwrap();
    cx.update(|cx| view.update(cx, |v, _| v.caret = at));
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |v, cx| {
            v.replace_and_mark_text_in_range(None, "ni", Some(2..2), window, cx);
            let marked = v.marked_text_range(window, cx).unwrap();
            let start16 = v.editor.read(cx).text().to_string()[..at]
                .encode_utf16()
                .count();
            assert_eq!(marked, start16..start16 + 2);
            assert_eq!(v.caret, at + 2);
            v.replace_and_mark_text_in_range(None, "你h", Some(2..2), window, cx);
            let bounds = v.bounds_for_range(marked.clone(), Bounds::default(), window, cx);
            assert!(bounds.is_some(), "the IME window can be placed");
            v.replace_text_in_range(None, "你好", window, cx);
            assert!(v.marked_text_range(window, cx).is_none());
        })
    })
    .unwrap();
    draw(cx, handle);
    let text = source(cx, &view);
    assert!(text.contains("中文 👍🏽 你好end."), "{text}");
    cx.update(|cx| assert_eq!(view.read(cx).caret, at + "你好".len()));
}

#[gpui_kit::test]
fn vertical_moves_keep_the_goal_x(cx: &mut TestAppContext) {
    let doc = "# A heading line that is big\n\nA paragraph of ordinary body text that runs along.\n\n- a list item with text\n";
    let (handle, view) = open(cx, doc);
    let start = doc.find("that").unwrap();
    cx.update(|cx| view.update(cx, |v, _| v.caret = start));
    draw(cx, handle);
    let mut ys = vec![];
    for key in ["down", "down", "up", "up"] {
        cx.simulate_keystrokes(handle, key);
        draw(cx, handle);
        ys.push(cx.update(|cx| view.read(cx).caret_position().unwrap().0.y));
    }
    assert!(ys[0] < ys[1], "down moves down: {ys:?}");
    assert!(ys[2] < ys[1] && ys[3] < ys[2], "up moves up: {ys:?}");
    cx.update(|cx| assert_eq!(view.read(cx).caret, start, "back to the same character"));
    let emoji = "x 👍🏽 **b** y\n";
    let (handle, view) = open(cx, emoji);
    cx.update(|cx| view.update(cx, |v, _| v.caret = 2));
    cx.simulate_keystrokes(handle, "right");
    cx.update(|cx| assert_eq!(view.read(cx).caret, 2 + "👍🏽".len()));
    cx.simulate_keystrokes(handle, "right right");
    cx.update(|cx| assert_eq!(view.read(cx).caret, emoji.find("b**").unwrap() + 1));
}

fn caret_is_valid(cx: &mut TestAppContext, view: &Entity<VisualView>, what: &str) {
    cx.update(|cx| {
        let v = view.read(cx);
        let len = v.editor.read(cx).text().len();
        assert!(v.caret <= len, "{what}: caret {} past {len}", v.caret);
        assert!(
            v.editor.read(cx).text().is_char_boundary(v.caret),
            "{what}: caret inside a character"
        );
        assert!(
            v.doc.locate(v.caret).is_some() || v.doc.blocks.iter().all(|b| b.leaves.is_empty()),
            "{what}: caret cannot be placed"
        );
    });
}

fn assert_synced(cx: &mut TestAppContext, view: &Entity<VisualView>, what: &str) {
    cx.update(|cx| {
        let v = view.read(cx);
        assert!(
            !v.parse_pending(),
            "{what}: a background parse is still pending"
        );
        let text = v.editor.read(cx).text().to_string();
        super::document::tests::assert_matches_full_parse(&v.doc, &text, what);
        assert_eq!(v.list.item_count(), v.rows.len(), "{what}: list items");
        let fresh = super::rows::Rows::build(&v.doc);
        assert_eq!(v.rows.rows, fresh.rows, "{what}: rows");
    });
}

#[gpui_kit::test]
fn undo_and_redo_work_across_views(cx: &mut TestAppContext) {
    let (handle, view) = open(cx, "one two\n\nthree\n");
    let primary = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    cx.update(|cx| view.update(cx, |v, _| v.caret = 3));
    cx.simulate_input(handle, "x");
    draw(cx, handle);
    assert_eq!(source(cx, &view), "onex two\n\nthree\n");
    cx.update_window(handle, |_, window, cx| {
        let editor = view.read(cx).editor.clone();
        editor.update(cx, |state, cx| {
            state.set_selected_range(10..10, cx);
            state.replace("Y", window, cx);
        });
    })
    .unwrap();
    draw(cx, handle);
    assert_eq!(source(cx, &view), "onex two\n\nYthree\n");
    assert_synced(cx, &view, "after source typing");
    cx.simulate_keystrokes(handle, &format!("{primary}-z"));
    draw(cx, handle);
    assert_eq!(source(cx, &view), "onex two\n\nthree\n");
    assert_synced(cx, &view, "undo 1");
    cx.simulate_keystrokes(handle, &format!("{primary}-z"));
    draw(cx, handle);
    assert_eq!(source(cx, &view), "one two\n\nthree\n");
    assert_synced(cx, &view, "undo 2");
    cx.update(|cx| assert_eq!(view.read(cx).caret, 3));
    cx.simulate_keystrokes(handle, &format!("{primary}-shift-z"));
    draw(cx, handle);
    assert_eq!(source(cx, &view), "onex two\n\nthree\n");
    assert_synced(cx, &view, "redo");
    caret_is_valid(cx, &view, "redo");
}

#[gpui_kit::test]
fn undo_restores_the_scroll_position(cx: &mut TestAppContext) {
    let text: String = (0..300).map(|i| format!("Paragraph {i}.\n\n")).collect();
    let (handle, view) = open(cx, &text);
    let at = text.find("Paragraph 150").unwrap() + 4;
    cx.update(|cx| {
        view.update(cx, |v, _| {
            v.caret = at;
            v.list.scroll_to_reveal_item(150);
        })
    });
    draw(cx, handle);
    let before = cx.update(|cx| view.read(cx).list.logical_scroll_top());
    cx.simulate_input(handle, "x");
    draw(cx, handle);
    cx.update(|cx| {
        view.read(cx).list.scroll_to(gpui_kit::ListOffset {
            item_ix: 0,
            offset_in_item: px(0.),
        })
    });
    draw(cx, handle);
    let primary = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    cx.simulate_keystrokes(handle, &format!("{primary}-z"));
    draw(cx, handle);
    assert_eq!(source(cx, &view), text);
    let after = cx.update(|cx| view.read(cx).list.logical_scroll_top());
    assert_eq!(after.item_ix, before.item_ix);
    cx.update(|cx| assert_eq!(view.read(cx).caret, at));
}

#[gpui_kit::test]
fn mixed_edits_keep_the_index_in_sync(cx: &mut TestAppContext) {
    let base = super::document::tests::SAMPLE.repeat(2)
        + &(0..40).map(|i| format!("- item {i}\n")).collect::<String>()
        + "\n```\ncode\n```\n";
    let (handle, view) = open(cx, &base);
    assert_synced(cx, &view, "after open");
    let pieces = [
        "x",
        "中",
        " ",
        "\n",
        "\n\n",
        "*",
        "```\n",
        "# ",
        "- ",
        "|",
        "<!--",
        "[a]: /u\n",
    ];
    let mut seed: u64 = 0x9E3779B97F4A7C15;
    let mut next = |n: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % n as u64) as usize
    };
    let primary = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    for step in 0..250 {
        let kind = next(10);
        let text = source(cx, &view);
        let mut at = next(text.len() + 1);
        while !text.is_char_boundary(at) {
            at -= 1;
        }
        let what = format!("step {step} kind {kind}");
        match kind {
            0..=2 => {
                cx.update(|cx| view.update(cx, |v, _| v.caret = v.doc.snap(at)));
                cx.simulate_input(handle, pieces[next(pieces.len())]);
            }
            3 => {
                cx.update(|cx| view.update(cx, |v, _| v.caret = v.doc.snap(at)));
                cx.simulate_keystrokes(handle, "backspace");
            }
            4 | 5 => {
                let mut end = (at + next(5)).min(text.len());
                while !text.is_char_boundary(end) {
                    end += 1;
                }
                let insert = pieces[next(pieces.len())];
                cx.update_window(handle, |_, window, cx| {
                    let editor = view.read(cx).editor.clone();
                    editor.update(cx, |state, cx| {
                        state.set_selected_range(at..end, cx);
                        state.replace(insert, window, cx);
                    });
                })
                .unwrap();
            }
            6 => cx.simulate_keystrokes(handle, &format!("{primary}-z")),
            7 => cx.simulate_keystrokes(handle, &format!("{primary}-shift-z")),
            8 => {
                let mut reloaded = text.clone();
                reloaded.insert_str(at, "\nreloaded line\n");
                cx.update_window(handle, |_, window, cx| {
                    let editor = view.read(cx).editor.clone();
                    editor.update(cx, |state, cx| state.set_value(reloaded, window, cx));
                })
                .unwrap();
            }
            _ => cx.simulate_keystrokes(handle, "down right"),
        }
        draw(cx, handle);
        cx.executor().advance_clock(super::view::SETTLE_AFTER * 2);
        draw(cx, handle);
        assert_synced(cx, &view, &what);
        caret_is_valid(cx, &view, &what);
    }
}

#[gpui_kit::test]
fn open_parses_a_prefix_then_the_rest_in_the_background(cx: &mut TestAppContext) {
    let text: String = (0..3000)
        .map(|i| format!("Paragraph {i} with *text*.\n\n"))
        .collect();
    cx.update(|cx| {
        gpui_kit::init(cx);
        super::view::init(cx);
    });
    let (handle, view) = cx.update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            let editor = cx.new(|cx| EditorState::new(window, cx).default_value(text.clone()));
            cx.new(|cx| VisualView::new(editor, window, cx))
        })
        .unwrap()
    });
    cx.update(|cx| {
        let v = view.read(cx);
        assert!(v.loading());
        assert!(!v.doc.blocks.is_empty(), "the first screen is parsed");
        assert!(v.doc.blocks.last().unwrap().end <= super::view::OPEN_PREFIX_BYTES + 200);
    });
    draw(cx, handle);
    assert_synced(cx, &view, "after open");
    cx.update(|cx| {
        let v = view.read(cx);
        assert!(!v.loading());
        assert_eq!(v.doc.blocks.len(), 3000);
    });
}

#[gpui_kit::test]
fn caret_deep_in_a_tall_block_is_revealed_and_typing_keeps_the_view(cx: &mut TestAppContext) {
    let paras: String = (0..40).map(|i| format!("Paragraph {i}.\n\n")).collect();
    let code: String = (0..300).map(|i| format!("line {i}\n")).collect();
    let text = format!("{paras}```\n{code}```\n\n{paras}");
    cx.update(|cx| {
        gpui_kit::init(cx);
        super::view::init(cx);
    });
    let (handle, view) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: Default::default(),
                    size: size(px(800.), px(600.)),
                })),
                ..Default::default()
            },
            cx,
            |window, cx| {
                let editor = cx.new(|cx| EditorState::new(window, cx).default_value(text.clone()));
                let view = cx.new(|cx| VisualView::new(editor, window, cx));
                view.update(cx, |v, cx| window.focus(&v.focus, cx));
                view
            },
        )
        .unwrap()
    });
    draw(cx, handle);
    let at = text.find("line 250").unwrap();
    cx.update(|cx| view.update(cx, |v, _| v.caret = at));
    cx.simulate_input(handle, "x");
    for _ in 0..4 {
        draw(cx, handle);
    }
    let visible = |cx: &mut TestAppContext| {
        cx.update(|cx| {
            let v = view.read(cx);
            let viewport = v.list.viewport_bounds();
            let (top, h, _) = v.caret_position().expect("caret laid out");
            eprintln!(
                "caret {:?} viewport {:?} scroll {:?}",
                top.y,
                viewport,
                v.list.logical_scroll_top()
            );
            top.y >= viewport.top() && top.y + h <= viewport.bottom()
        })
    };
    assert!(visible(cx), "caret revealed");
    let before = cx.update(|cx| view.read(cx).caret_position().unwrap().0.y);
    for _ in 0..5 {
        cx.simulate_input(handle, "y");
        draw(cx, handle);
    }
    let after = cx.update(|cx| view.read(cx).caret_position().unwrap().0.y);
    assert!(
        (after - before).abs() < px(1.),
        "the view stays put while typing: {before:?} → {after:?}"
    );
}

fn select(cx: &mut TestAppContext, view: &Entity<VisualView>, range: std::ops::Range<usize>) {
    cx.update(|cx| {
        view.update(cx, |v, cx| {
            v.anchor = Some(range.start);
            v.caret = range.end;
            cx.notify();
        })
    });
}

const MOD: &str = if cfg!(target_os = "macos") {
    "cmd"
} else {
    "ctrl"
};

#[gpui_kit::test]
fn selection_copy_and_replace(cx: &mut TestAppContext) {
    let doc = "Some **bold** text here.\n";
    let (handle, view) = open(cx, doc);
    let at = doc.find("bold").unwrap();
    cx.update(|cx| view.update(cx, |v, _| v.caret = at - 3));
    cx.simulate_keystrokes(
        handle,
        "shift-right shift-right shift-right shift-right shift-right",
    );
    draw(cx, handle);
    let selection = cx.update(|cx| view.read(cx).selection());
    let selected = &doc[selection.clone()];
    assert!(selected.starts_with(" **bold"), "{selected:?}");
    cx.simulate_keystrokes(handle, &format!("{MOD}-c"));
    let clip = cx.update(|cx| cx.read_from_clipboard().and_then(|c| c.text()));
    assert_eq!(clip.as_deref(), Some(selected));
    cx.simulate_keystrokes(handle, "left");
    cx.update(|cx| {
        let v = view.read(cx);
        assert_eq!((v.caret, v.anchor), (selection.start, None));
    });
    select(cx, &view, at..at + 4);
    cx.simulate_input(handle, "z");
    draw(cx, handle);
    assert_eq!(source(cx, &view), "Some **z** text here.\n");
    cx.simulate_keystrokes(handle, &format!("{MOD}-a backspace"));
    draw(cx, handle);
    assert_eq!(source(cx, &view), "");
    assert_synced(cx, &view, "after deleting everything");
}

#[gpui_kit::test]
fn formatting_shortcuts_toggle_markdown(cx: &mut TestAppContext) {
    let doc = "First paragraph.\n\nSecond word here.\n\nThird.\n";
    let (handle, view) = open(cx, doc);
    let at = doc.find("word").unwrap();
    select(cx, &view, at..at + 4);
    cx.simulate_keystrokes(handle, &format!("{MOD}-b"));
    draw(cx, handle);
    assert_eq!(source(cx, &view), doc.replace("word", "**word**"));
    cx.simulate_keystrokes(handle, &format!("{MOD}-b"));
    draw(cx, handle);
    assert_eq!(source(cx, &view), doc);
    cx.simulate_keystrokes(handle, &format!("{MOD}-2"));
    draw(cx, handle);
    assert_eq!(source(cx, &view), doc.replace("Second", "## Second"));
    assert_synced(cx, &view, "after a heading toggle");
    cx.simulate_keystrokes(handle, &format!("{MOD}-shift-8"));
    draw(cx, handle);
    assert_eq!(source(cx, &view), doc.replace("Second", "- Second"));
    cx.simulate_keystrokes(handle, &format!("{MOD}-z"));
    draw(cx, handle);
    assert_eq!(source(cx, &view), doc.replace("Second", "## Second"));
    assert_synced(cx, &view, "after undoing a list toggle");
}

#[gpui_kit::test]
fn enter_continues_lists(cx: &mut TestAppContext) {
    for (doc, typed, expected) in [
        ("- one\n", "two", "- one\n- two\n"),
        ("- [ ] one\n", "two", "- [ ] one\n- [ ] two\n"),
        ("1. one\n", "two", "1. one\n2. two\n"),
    ] {
        let (handle, view) = open(cx, doc);
        cx.update(|cx| view.update(cx, |v, _| v.caret = doc.len() - 1));
        cx.simulate_keystrokes(handle, "enter");
        cx.simulate_input(handle, typed);
        draw(cx, handle);
        assert_eq!(source(cx, &view), expected);
        assert_synced(cx, &view, "after continuing a list");
        cx.simulate_keystrokes(handle, "enter enter");
        draw(cx, handle);
        let text = source(cx, &view);
        assert!(text.starts_with(expected.trim_end()), "{text:?}");
        assert!(
            !text.trim_end().ends_with(['-', ']', '.']),
            "the empty item ends the list: {text:?}"
        );
    }
}

#[gpui_kit::test]
fn word_and_paragraph_selection(cx: &mut TestAppContext) {
    let doc = "Alpha beta gamma.\n\nNext.\n";
    let (handle, view) = open(cx, doc);
    let at = doc.find("beta").unwrap() + 1;
    cx.update(|cx| {
        view.update(cx, |v, cx| {
            v.caret = at;
            v.select_word(cx);
        })
    });
    let selection = cx.update(|cx| view.read(cx).selection());
    assert_eq!(&doc[selection], "beta");
    cx.update(|cx| view.update(cx, |v, cx| v.select_paragraph(cx)));
    let selection = cx.update(|cx| view.read(cx).selection());
    assert_eq!(&doc[selection], "Alpha beta gamma.");
    draw(cx, handle);
    cx.update(|cx| {
        assert!(view.read(cx).caret_position().is_some());
    });
}

#[gpui_kit::test]
fn reference_definitions_update_every_link(cx: &mut TestAppContext) {
    let doc = "See [the docs][d] here.\n\nMiddle text.\n\nAnd [again][D] there.\n\n[d]: https://one.example\n";
    let (handle, view) = open(cx, doc);
    let link_point = |cx: &mut TestAppContext, word: &str| {
        cx.update(|cx| {
            let v = view.read(cx);
            let src = doc.find(word).unwrap() + 1;
            let (bix, lix, index) = v.doc.locate(src).unwrap();
            let key = v.doc.blocks[bix].leaves[lix].key;
            let geometry = v.geometry.borrow();
            let leaf = &geometry[&key];
            let p = leaf.position_for_index(index);
            point(p.x, p.y + leaf.line_height() / 2.)
        })
    };
    let secondary = cx.update(|cx| gpui_kit::Modifiers::secondary_key(cx));
    let follow = |cx: &mut TestAppContext, word: &str| {
        let at = link_point(cx, word);
        click(cx, handle, at, 1, secondary);
        draw(cx, handle);
        cx.opened_url()
    };
    let at = link_point(cx, "docs");
    click(cx, handle, at, 1, Default::default());
    draw(cx, handle);
    assert_eq!(cx.opened_url(), None);
    let caret = cx.update(|cx| view.read(cx).caret);
    assert!((doc.find("docs").unwrap()..doc.find("docs").unwrap() + 5).contains(&caret));
    assert_eq!(follow(cx, "again").as_deref(), Some("https://one.example"));
    let edit = |cx: &mut TestAppContext, from: &str, to: &str| {
        cx.update_window(handle, |_, window, cx| {
            let editor = view.read(cx).editor.clone();
            editor.update(cx, |e, cx| {
                let at = e.text().to_string().find(from).unwrap();
                e.set_selected_range(at..at + from.len(), cx);
                e.replace(to.to_string(), window, cx);
            });
        })
        .unwrap();
        draw(cx, handle);
        draw(cx, handle);
    };
    edit(cx, "one.", "two.");
    assert_eq!(follow(cx, "docs").as_deref(), Some("https://two.example"));
    edit(cx, "two.", "three.");
    assert_eq!(
        follow(cx, "again").as_deref(),
        Some("https://three.example")
    );
}

fn click(
    cx: &mut TestAppContext,
    handle: AnyWindowHandle,
    position: gpui_kit::Point<gpui_kit::Pixels>,
    click_count: usize,
    modifiers: gpui_kit::Modifiers,
) {
    use gpui_kit::InputEvent as _;
    cx.update_window(handle, |_, window, cx| {
        window.dispatch_event(
            gpui_kit::MouseDownEvent {
                button: gpui_kit::MouseButton::Left,
                position,
                modifiers,
                click_count,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(
            gpui_kit::MouseUpEvent {
                button: gpui_kit::MouseButton::Left,
                position,
                modifiers,
                click_count,
            }
            .to_platform_input(),
            cx,
        );
    })
    .unwrap();
}

fn leaf_top(
    cx: &mut TestAppContext,
    view: &Entity<VisualView>,
    src: usize,
) -> Option<gpui_kit::Pixels> {
    cx.update(|cx| {
        let v = view.read(cx);
        let (bix, lix, _) = v.doc.locate(src)?;
        let key = v.doc.blocks[bix].leaves[lix].key;
        let top = v.geometry.borrow().get(&key)?.bounds().top();
        Some(top)
    })
}

#[gpui_kit::test]
fn typing_does_not_shift_the_layout(cx: &mut TestAppContext) {
    let doc: String = (0..12).map(|i| format!("Para {i} short.\n\n")).collect();
    let (handle, view) = open(cx, &doc);
    let starts: Vec<usize> = (0..12)
        .map(|i| doc.find(&format!("Para {i} ")).unwrap())
        .collect();
    let tops = |cx: &mut TestAppContext, text: &str| -> Vec<gpui_kit::Pixels> {
        (0..12)
            .map(|i| {
                let at = text.find(&format!("Para {i} ")).unwrap();
                leaf_top(cx, &view, at).unwrap_or_else(|| {
                    let n = cx.update(|cx| view.read(cx).geometry.borrow().len());
                    let l = cx.update(|cx| view.read(cx).doc.locate(at));
                    panic!("no geometry for {i} at {at}: {n} leaves, locate {l:?}")
                })
            })
            .collect()
    };
    draw(cx, handle);
    let before = tops(cx, &doc);
    cx.update(|cx| view.update(cx, |v, _| v.caret = starts[5] + 7));
    cx.simulate_input(handle, "xyz");
    draw(cx, handle);
    let text = source(cx, &view);
    assert_eq!(tops(cx, &text), before, "typing in a line moves nothing");
    cx.simulate_keystrokes(handle, "end enter");
    cx.simulate_input(handle, "New");
    draw(cx, handle);
    let text = source(cx, &view);
    let after = tops(cx, &text);
    assert_eq!(after[..6], before[..6], "above and at the edit: still");
    let shift = after[6] - before[6];
    assert!(shift > px(10.), "below moves down by the new paragraph");
    for i in 6..12 {
        assert_eq!(after[i] - before[i], shift, "below moves together");
    }
}

#[gpui_kit::test]
fn edits_above_the_viewport_keep_the_view_anchored(cx: &mut TestAppContext) {
    let doc: String = (0..200)
        .map(|i| format!("Paragraph {i} of the anchored test.\n\n"))
        .collect();
    let (handle, view) = open(cx, &doc);
    let target = doc.find("Paragraph 150 ").unwrap();
    cx.update(|cx| view.update(cx, |v, cx| v.place_caret(target, Some(px(300.)), cx)));
    for _ in 0..3 {
        draw(cx, handle);
    }
    let before = leaf_top(cx, &view, target).expect("in view");
    let at = doc.find("Paragraph 3 ").unwrap();
    cx.update_window(handle, |_, window, cx| {
        let editor = view.read(cx).editor.clone();
        editor.update(cx, |e, cx| {
            e.set_selected_range(at..at, cx);
            e.replace(
                "One.\n\nTwo.\n\nThree.\n\nFour.\n\nFive.\n\n".to_string(),
                window,
                cx,
            );
        });
    })
    .unwrap();
    draw(cx, handle);
    draw(cx, handle);
    let moved = target + "One.\n\nTwo.\n\nThree.\n\nFour.\n\nFive.\n\n".len();
    let after = leaf_top(cx, &view, moved).expect("still in view");
    assert_eq!(after, before);
}

fn set_prefs(cx: &mut TestAppContext, change: impl FnOnce(&mut super::prefs::WritingPrefs)) {
    cx.update(|cx| {
        let mut prefs = super::prefs::WritingPrefs::get(cx);
        change(&mut prefs);
        cx.set_global(prefs);
    });
}

fn mouse(
    cx: &mut TestAppContext,
    handle: AnyWindowHandle,
    position: gpui_kit::Point<gpui_kit::Pixels>,
    phase: u8,
) {
    use gpui_kit::InputEvent as _;
    cx.update_window(handle, |_, window, cx| {
        let event = match phase {
            0 => gpui_kit::MouseDownEvent {
                button: gpui_kit::MouseButton::Left,
                position,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            1 => gpui_kit::MouseMoveEvent {
                position,
                pressed_button: Some(gpui_kit::MouseButton::Left),
                modifiers: Default::default(),
            }
            .to_platform_input(),
            _ => gpui_kit::MouseUpEvent {
                button: gpui_kit::MouseButton::Left,
                position,
                modifiers: Default::default(),
                click_count: 1,
            }
            .to_platform_input(),
        };
        window.dispatch_event(event, cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn typewriter_holds_the_caret_line_at_the_guide(cx: &mut TestAppContext) {
    let doc: String = (0..80)
        .map(|i| format!("Line {i} of the draft.\n\n"))
        .collect();
    let (handle, view) = open(cx, &doc);
    set_prefs(cx, |p| {
        p.typewriter = true;
        p.typewriter_guide = 0.5;
    });
    draw(cx, handle);
    draw(cx, handle);
    let at = doc.find("Line 40 ").unwrap() + 4;
    cx.update(|cx| view.update(cx, |v, cx| v.place_caret(at, None, cx)));
    cx.simulate_keystrokes(handle, "end");
    let mut ys = vec![];
    for _ in 0..6 {
        cx.simulate_keystrokes(handle, "enter");
        cx.simulate_input(handle, "More");
        draw(cx, handle);
        draw(cx, handle);
        let (y, guide) = cx.update(|cx| {
            let v = view.read(cx);
            (v.caret_viewport_y().unwrap(), v.guide_y(cx).unwrap())
        });
        assert!((y - guide).abs() <= px(1.), "caret {y:?} guide {guide:?}");
        ys.push(y);
    }
    assert!(
        ys.windows(2).all(|w| (w[0] - w[1]).abs() <= px(1.)),
        "{ys:?}"
    );
    let first = doc.find("Line 0 ").unwrap();
    cx.update(|cx| view.update(cx, |v, cx| v.place_caret(first, None, cx)));
    cx.simulate_keystrokes(handle, "right");
    draw(cx, handle);
    draw(cx, handle);
    let (y, guide) = cx.update(|cx| {
        let v = view.read(cx);
        (v.caret_viewport_y().unwrap(), v.guide_y(cx).unwrap())
    });
    assert!(
        (y - guide).abs() <= px(BODY_LINE),
        "the first line reaches the guide: {y:?} {guide:?}"
    );
}

const BODY_LINE: f32 = 40.;

#[gpui_kit::test]
fn the_typewriter_guide_drags_and_snaps_to_center(cx: &mut TestAppContext) {
    let doc: String = (0..40).map(|i| format!("Line {i}.\n\n")).collect();
    let (handle, view) = open(cx, &doc);
    set_prefs(cx, |p| {
        p.typewriter = true;
        p.typewriter_guide = 0.5;
    });
    draw(cx, handle);
    draw(cx, handle);
    let (viewport, guide) = cx.update(|cx| {
        let v = view.read(cx);
        (v.list.viewport_bounds(), v.guide_line_y(cx).unwrap())
    });
    let x = viewport.center().x;
    let start = point(x, viewport.top() + guide);
    mouse(cx, handle, start, 0);
    let line = cx.update(|cx| view.read(cx).caret_position().unwrap().1);
    let lower = point(x, viewport.top() + viewport.size.height * 0.7 + line);
    mouse(cx, handle, lower, 1);
    draw(cx, handle);
    cx.update(|cx| {
        assert!((view.read(cx).guide_fraction(cx).unwrap() - 0.7).abs() < 0.01);
    });
    mouse(cx, handle, lower, 2);
    draw(cx, handle);
    cx.update(|cx| {
        let prefs = super::prefs::WritingPrefs::get(cx);
        assert!((prefs.typewriter_guide - 0.7).abs() < 0.01);
    });
    let guide = cx.update(|cx| view.read(cx).guide_line_y(cx).unwrap());
    mouse(cx, handle, point(x, viewport.top() + guide), 0);
    let near = point(x, viewport.top() + viewport.size.height * 0.515 + line);
    mouse(cx, handle, near, 1);
    mouse(cx, handle, near, 2);
    draw(cx, handle);
    cx.update(|cx| {
        assert_eq!(super::prefs::WritingPrefs::get(cx).typewriter_guide, 0.5);
    });
}

#[gpui_kit::test]
fn focus_follows_the_sentence_and_fades_between_them(cx: &mut TestAppContext) {
    let doc = "First sentence here. Dr. Who arrives next. Last one\n\nAnother paragraph.\n";
    let (handle, view) = open(cx, doc);
    set_prefs(cx, |p| p.focus = super::prefs::FocusMode::Sentence);
    let at = doc.find("arrives").unwrap();
    cx.update(|cx| view.update(cx, |v, cx| v.place_caret(at, None, cx)));
    draw(cx, handle);
    let target = cx.update(|cx| {
        view.read(cx)
            .focus_target(super::prefs::FocusMode::Sentence)
    });
    let target = target.unwrap();
    let text = cx.update(|cx| {
        let v = view.read(cx);
        let (bix, lix) = v.doc.leaf_by_key(target.key).unwrap();
        v.doc.blocks[bix].leaves[lix].text[target.range.clone()].to_string()
    });
    assert_eq!(text, "Dr. Who arrives next.");
    let before = cx.update(|cx| view.read(cx).focus_fade_active().cloned());
    cx.simulate_input(handle, "x");
    draw(cx, handle);
    cx.update(|cx| {
        let v = view.read(cx);
        let after = v.focus_fade_active().cloned();
        assert!(after.is_none() || after == before, "typing starts no fade");
        let now = v.focus_target(super::prefs::FocusMode::Sentence).unwrap();
        assert_eq!(
            (now.block, now.range.start),
            (target.block, target.range.start)
        );
    });
    let next = cx.update(|cx| view.read(cx).caret) + "rrives next. L".len();
    cx.update(|cx| view.update(cx, |v, cx| v.place_caret(next, None, cx)));
    draw(cx, handle);
    cx.update(|cx| assert!(view.read(cx).focus_fade_active().is_some(), "moving fades"));
    cx.update(|cx| cx.set_reduce_motion(true));
    let back = doc.find("First").unwrap();
    cx.update(|cx| view.update(cx, |v, cx| v.place_caret(back, None, cx)));
    draw(cx, handle);
    cx.update(|cx| {
        assert!(
            view.read(cx).focus_fade_active().is_none(),
            "reduced motion switches at once"
        )
    });
    set_prefs(cx, |p| p.focus = super::prefs::FocusMode::Paragraph);
    draw(cx, handle);
    let range = cx.update(|cx| {
        view.read(cx)
            .focus_target(super::prefs::FocusMode::Paragraph)
            .unwrap()
            .range
    });
    assert_eq!(range.start, 0);
    assert!(range.end > "First sentence here. Dr. Who arrives next. Last one".len() - 1);
}
