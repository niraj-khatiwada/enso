#[path = "../src/markdown_edit.rs"]
#[allow(dead_code)]
mod markdown_edit;
#[path = "../src/tokens.rs"]
#[allow(dead_code)]
mod tokens;
#[path = "../src/visual/mod.rs"]
#[allow(dead_code)]
mod visual;

use std::time::{Duration, Instant};

use gpui_kit::component::input::EditorState;
use gpui_kit::*;
use visual::perf::{Sample, ms, percentiles};
use visual::view::VisualView;

const BUDGET_120: Duration = Duration::from_micros(8_333);
const BUDGET_60: Duration = Duration::from_micros(16_667);
const STEP: Duration = Duration::from_millis(40);

fn corpus(target: usize) -> String {
    let mut out = String::from("---\ntitle: Benchmark corpus\ntags: [bench]\n---\n\n");
    let words = [
        "the", "quiet", "page", "holds", "a", "thought", "until", "it", "is", "ready", "to",
        "leave", "writing", "waits", "then", "rushes",
    ];
    let mut seed: u64 = 7;
    let mut next = |n: usize| {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((seed >> 33) % n as u64) as usize
    };
    let sentence = |next: &mut dyn FnMut(usize) -> usize| {
        let n = 8 + next(10);
        let mut s: Vec<String> = (0..n)
            .map(|_| words[next(words.len())].to_string())
            .collect();
        let i = next(n);
        s[i] = match next(5) {
            0 => format!("**{}**", s[i]),
            1 => format!("*{}*", s[i]),
            2 => format!("`{}`", s[i]),
            3 => format!("[{}](https://example.com)", s[i]),
            _ => s[i].clone(),
        };
        let mut s = s.join(" ");
        s[..1].make_ascii_uppercase();
        s + "."
    };
    let mut k = 0;
    let large_at = target / 2;
    let mut large_done = target < 20_000;
    while out.len() < target {
        k += 1;
        out += &format!("## Section {k}\n\n");
        out += &format!(
            "{} {} {}\n{} {}\n\n",
            sentence(&mut next),
            sentence(&mut next),
            sentence(&mut next),
            sentence(&mut next),
            sentence(&mut next)
        );
        out += "नमस्ते दुनिया। यह एक वाक्य है। 中文的句子在这里。 Emoji 👍🏽 and 👩‍👩‍👧.\n\n";
        out += &format!(
            "- {}\n- [ ] {}\n  - nested {}\n- {}\n\n",
            sentence(&mut next),
            sentence(&mut next),
            sentence(&mut next),
            sentence(&mut next)
        );
        out += &format!("> {}\n> {}\n\n", sentence(&mut next), sentence(&mut next));
        if k % 3 == 0 {
            out += "```rust\nfn main() {\n    println!(\"hello {}\", 42);\n}\n```\n\n";
        }
        if k % 4 == 0 {
            out +=
                "| Key | Value | 中文 |\n|---|:--:|--:|\n| a | **b** | 中 |\n| c | `d` | 👍🏽 |\n\n";
        }
        if !large_done && out.len() > large_at {
            large_done = true;
            out += "## Large list\n\n";
            for i in 0..400 {
                out += &format!("- list item {i}: {}\n", sentence(&mut next));
            }
            out += "\n## Large table\n\n| # | Name | Note |\n|---|---|---|\n";
            for i in 0..400 {
                out += &format!("| {i} | row {i} | {} |\n", sentence(&mut next));
            }
            out += "\n";
        }
    }
    out
}

#[derive(Clone)]
enum Step {
    Case(&'static str),
    CaretAt(String, usize),
    CaretFraction(f32),
    Keys(&'static str, usize),
    Input(String),
    Scroll(usize, f32),
    JumpToEnd,
}

fn script(size: usize) -> Vec<Step> {
    use Step::*;
    let primary = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    let undo: &'static str = Box::leak(format!("{primary}-z").into_boxed_str());
    let redo: &'static str = Box::leak(format!("{primary}-shift-z").into_boxed_str());
    let mut s = vec![
        Case("type: start"),
        CaretAt("## Section 1\n\n".into(), 16),
        Keys("a", 40),
        Case("type: middle"),
        CaretFraction(0.5),
        Keys("a", 40),
        Case("type: end"),
        CaretFraction(0.97),
        Keys("a", 40),
        Case("fence: open near top"),
        CaretAt("## Section 2\n\n".into(), 14),
        Keys("`", 3),
        Keys("enter", 1),
        Case("fence: type inside unclosed"),
        Keys("x", 30),
        Case("fence: close"),
        Keys("enter", 1),
        Keys("`", 3),
        Keys("enter", 1),
        Case("setext underline"),
        CaretAt("## Section 3\n\n".into(), 14),
        Keys("end", 1),
        Input("\n--".into()),
        Keys("-", 1),
        Keys("backspace", 4),
        Case("html comment opener"),
        CaretAt("## Section 4\n\n".into(), 14),
        Keys("<", 1),
        Keys("!", 1),
        Keys("-", 2),
        Keys("backspace", 4),
        Case("link reference definition"),
        CaretAt("## Section 5\n\n".into(), 14),
        Input("[ref]: https://example.com\n\n".into()),
        Keys("x", 10),
    ];
    if size >= 20_000 {
        s.extend([
            Case("large list: type in item"),
            CaretAt("- list item 200: ".into(), 18),
            Keys("a", 30),
            Case("large table: type in cell"),
            CaretAt("| 200 | row 200".into(), 10),
            Keys("a", 30),
        ]);
    }
    s.extend([
        Case("paste 20 KiB"),
        CaretFraction(0.3),
        Input("Pasted line of ordinary prose for the benchmark.\n\n".repeat(400)),
        Case("undo large paste"),
        Keys(undo, 1),
        Case("redo large paste"),
        Keys(redo, 1),
        Case("scroll: trackpad deltas"),
        Scroll(if size > 100_000 { 400 } else { 200 }, 60.),
        Case("scroll: jump to end"),
        JumpToEnd,
    ]);
    s
}

fn report(size: &str, view: &VisualView, scroll_frames: &[(String, Vec<Duration>)]) {
    let perf = view.perf.borrow();
    let mut labels: Vec<&'static str> = Vec::new();
    for s in &perf.samples {
        if !labels.contains(&s.label) {
            labels.push(s.label);
        }
    }
    println!("\n### {size} ({} bytes)\n", view.editor_len_hint());
    println!(
        "| Case | n | work p50 | p95 | p99 | max | >8.33 | >16.67 | wait p50 | worst input: parsed bytes, path |"
    );
    println!("|---|---|---|---|---|---|---|---|---|---|");
    for label in labels {
        let samples: Vec<&Sample> = perf.samples.iter().filter(|s| s.label == label).collect();
        let mut work: Vec<f64> = samples.iter().map(|s| ms(s.work())).collect();
        let mut wait: Vec<f64> = samples.iter().map(|s| ms(s.wait)).collect();
        let Some([p50, p95, p99, max, _]) = percentiles(&mut work) else {
            continue;
        };
        let wait50 = percentiles(&mut wait).map_or(0., |p| p[0]);
        let over = |b: Duration| samples.iter().filter(|s| s.work() > b).count();
        let worst = samples.iter().max_by_key(|s| s.work()).unwrap();
        println!(
            "| {label} | {} | {p50:.2} | {p95:.2} | {p99:.2} | {max:.2} | {} | {} | {wait50:.2} | {:.2} ms: {} B, {:?} |",
            samples.len(),
            over(BUDGET_120),
            over(BUDGET_60),
            ms(worst.work()),
            worst.parsed_bytes,
            worst.path,
        );
    }
    for (label, frames) in scroll_frames {
        let mut f: Vec<f64> = frames.iter().map(|d| ms(*d)).collect();
        if let Some([p50, p95, p99, max, _]) = percentiles(&mut f) {
            let over = |b: Duration| frames.iter().filter(|d| **d > b).count();
            println!(
                "| {label} (per frame) | {} | {p50:.2} | {p95:.2} | {p99:.2} | {max:.2} | {} | {} | – | – |",
                frames.len(),
                over(BUDGET_120),
                over(BUDGET_60),
            );
        }
    }
}

impl VisualView {
    fn editor_len_hint(&self) -> usize {
        self.doc.blocks.last().map_or(0, |b| b.end)
    }
}

fn main() {
    let size_arg = std::env::args().nth(1).unwrap_or_else(|| "200k".into());
    let target = match size_arg.as_str() {
        "1k" => 1024,
        "50k" => 50 * 1024,
        "200k" => 200 * 1024,
        "255k" => 255 * 1024 - 2048,
        other => other.parse().expect("1k | 50k | 200k | 255k | <bytes>"),
    };
    let text = corpus(target);
    let _ = std::fs::create_dir_all("target/bench");
    let _ = std::fs::write(format!("target/bench/{size_arg}.md"), &text);
    let log = format!("target/bench/{size_arg}.jsonl");
    let _ = std::fs::remove_file(&log);
    unsafe { std::env::set_var("ENSO_PERF_LOG", &log) };
    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            visual::view::init(cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1000.), px(780.)),
                    cx,
                ))),
                ..Default::default()
            };
            let opened = Instant::now();
            let len = text.len();
            let mut editor_time = Duration::ZERO;
            let mut view_time = Duration::ZERO;
            let (handle, view) = gpui_kit::open_window(options, cx, |window, cx| {
                let t = Instant::now();
                let editor = cx.new(|cx| {
                    EditorState::new(window, cx)
                        .language("markdown")
                        .folding(false)
                        .default_value(text)
                });
                editor_time = t.elapsed();
                let t = Instant::now();
                let view = cx.new(|cx| VisualView::new(editor, window, cx));
                view_time = t.elapsed();
                view.update(cx, |view, cx| window.focus(&view.focus, cx));
                view
            })
            .expect("a window");
            let constructed = opened.elapsed();
            cx.spawn(async move |cx| {
                let mut first_frame = None;
                let mut loaded = None;
                for _ in 0..2000 {
                    let (frames, loading) = cx.update(|cx| {
                        let v = view.read(cx);
                        (v.perf.borrow().frames.len(), v.loading() || v.parse_pending())
                    });
                    if frames > 0 && first_frame.is_none() {
                        first_frame = Some(opened.elapsed());
                    }
                    if frames > 0 && !loading {
                        loaded = Some(opened.elapsed());
                        break;
                    }
                    cx.background_executor().timer(Duration::from_millis(2)).await;
                }
                println!(
                    "\nOpen {size_arg} ({len} bytes): main thread {:.1} ms (EditorState {:.1} ms, VisualView {:.1} ms), first frame {:.1} ms, fully parsed {:.1} ms",
                    ms(constructed),
                    ms(editor_time),
                    ms(view_time),
                    first_frame.map_or(f64::NAN, ms),
                    loaded.map_or(f64::NAN, ms),
                );
                cx.update(|cx| view.read(cx).perf.borrow_mut().reset());
                let mut scroll_frames: Vec<(String, Vec<Duration>)> = Vec::new();
                let mut label = "";
                let only = std::env::var("BENCH_ONLY").ok();
                let mut steps = Vec::new();
                let mut keep = only.is_none();
                for step in script(len) {
                    if let Step::Case(l) = &step {
                        keep = only.as_deref().is_none_or(|o| l.contains(o));
                    }
                    if keep {
                        steps.push(step);
                    }
                }
                for step in steps {
                    match step {
                        Step::Case(l) => {
                            label = l;
                            cx.update(|cx| view.read(cx).perf.borrow_mut().label = l);
                        }
                        Step::CaretAt(needle, plus) => {
                            let _ = handle.update(cx, |_, _, cx| {
                                view.update(cx, |v, cx| {
                                    let text = v.editor.read(cx).text().to_string();
                                    if let Some(at) = text.find(&needle) {
                                        v.caret = v.doc.snap(at + plus);
                                    }
                                    cx.notify();
                                })
                            });
                        }
                        Step::CaretFraction(f) => {
                            let _ = handle.update(cx, |_, _, cx| {
                                view.update(cx, |v, cx| {
                                    let len = v.editor.read(cx).text().len();
                                    v.caret = v.doc.snap((len as f32 * f) as usize);
                                    cx.notify();
                                })
                            });
                        }
                        Step::Keys(key, times) => {
                            for _ in 0..times {
                                let _ = handle.update(cx, |_, window, cx| {
                                    window.dispatch_keystroke(Keystroke::parse(key).unwrap(), cx);
                                });
                                cx.background_executor().timer(STEP).await;
                            }
                            if std::env::var_os("ENSO_BENCH_DEBUG").is_some() {
                                cx.background_executor().timer(Duration::from_millis(500)).await;
                                let (len, n, caret, shown) = cx.update(|cx| {
                                    let v = view.read(cx);
                                    (
                                        v.editor.read(cx).text().len(),
                                        v.perf.borrow().samples.len(),
                                        v.caret,
                                        v.caret_position().map(|p| p.0),
                                    )
                                });
                                eprintln!("after {key}: text {len} bytes, {n} samples, caret {caret} at {shown:?}");
                            }
                        }
                        Step::Input(text) => {
                            let _ = handle.update(cx, |_, window, cx| {
                                view.update(cx, |v, cx| {
                                    v.perf.borrow_mut().input();
                                    v.replace_text_in_range(None, &text, window, cx);
                                })
                            });
                        }
                        Step::Scroll(count, delta) => {
                            let start = cx.update(|cx| view.read(cx).perf.borrow().frames.len());
                            for _ in 0..count {
                                let _ = handle.update(cx, |_, window, cx| {
                                    view.update(cx, |v, cx| {
                                        v.list.scroll_by(px(delta));
                                        cx.notify();
                                    });
                                    window.refresh();
                                });
                                cx.background_executor().timer(Duration::from_millis(17)).await;
                            }
                            let frames = cx.update(|cx| view.read(cx).perf.borrow().frames[start..].to_vec());
                            scroll_frames.push((label.to_string(), frames));
                        }
                        Step::JumpToEnd => {
                            let start = cx.update(|cx| view.read(cx).perf.borrow().frames.len());
                            let _ = handle.update(cx, |_, window, cx| {
                                view.update(cx, |v, cx| {
                                    let n = v.list.item_count();
                                    v.list.scroll_to_reveal_item(n.saturating_sub(1));
                                    cx.notify();
                                });
                                window.refresh();
                            });
                            cx.background_executor().timer(Duration::from_millis(200)).await;
                            let frames = cx.update(|cx| view.read(cx).perf.borrow().frames[start..].to_vec());
                            scroll_frames.push((label.to_string(), frames));
                        }
                    }
                    cx.background_executor().timer(STEP).await;
                }
                cx.background_executor().timer(Duration::from_millis(1500)).await;
                cx.update(|cx| {
                    report(&size_arg, view.read(cx), &scroll_frames);
                    cx.quit();
                });
            })
            .detach();
        });
}
