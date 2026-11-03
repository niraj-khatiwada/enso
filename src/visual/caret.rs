use std::time::Duration;

use gpui_kit::{Context, Task};

use super::view::VisualView;

const BLINK: Duration = Duration::from_millis(530);

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
        if std::env::var_os("ENSO_NO_BLINK").is_some() {
            return;
        }
        let epoch = self.epoch;
        self._task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(BLINK).await;
                let alive = this.update(cx, |view, cx| {
                    if view.caret_blink.epoch != epoch {
                        return false;
                    }
                    view.caret_blink.visible = !view.caret_blink.visible;
                    cx.notify();
                    true
                });
                if !matches!(alive, Ok(true)) {
                    break;
                }
            }
        }));
    }
}
