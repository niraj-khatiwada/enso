use std::time::Duration;

use gpui_kit::{Context, Task};

use super::prefs::{BLINK_CELLS, WritingPrefs};
use super::view::VisualView;

pub struct Caret {
    visible: bool,
    epoch: usize,
    _task: Option<Task<()>>,
}

impl Caret {
    pub fn new(cx: &mut Context<VisualView>) -> Self {
        let _ = cx;
        Self {
            visible: true,
            epoch: 0,
            _task: None,
        }
    }

    pub fn visible(&self) -> bool {
        self.visible
    }

    pub fn reset(&mut self, cx: &mut Context<VisualView>) {
        self.visible = true;
        self.epoch += 1;
        let prefs = WritingPrefs::get(cx);
        if !prefs.caret_blink || std::env::var_os("ENSO_SPIKE_NO_BLINK").is_some() {
            self._task = None;
            return;
        }
        let epoch = self.epoch;
        self._task = Some(cx.spawn(async move |this, cx| {
            let mut cell = 0u32;
            loop {
                let step = this
                    .update(cx, |_, cx| {
                        let prefs = WritingPrefs::get(cx);
                        Duration::from_millis((prefs.blink_period_ms / BLINK_CELLS).max(16) as u64)
                    })
                    .unwrap_or(Duration::from_millis(88));
                cx.background_executor().timer(step).await;
                cell = (cell + 1) % BLINK_CELLS;
                let alive = this.update(cx, |view, cx| {
                    if view.caret_blink.epoch != epoch {
                        return false;
                    }
                    let shown = WritingPrefs::get(cx).shows_in(cell);
                    if shown != view.caret_blink.visible {
                        view.caret_blink.visible = shown;
                        cx.notify();
                    }
                    true
                });
                if !matches!(alive, Ok(true)) {
                    break;
                }
            }
        }));
    }
}
