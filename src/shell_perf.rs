use std::{
    io::Write as _,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use gpui_kit::*;

struct Log {
    file: Mutex<std::fs::File>,
}

fn log() -> Option<&'static Log> {
    static LOG: OnceLock<Option<Log>> = OnceLock::new();
    LOG.get_or_init(|| {
        let path = std::env::var_os("ENSO_PERF_LOG")?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .ok()?;
        Some(Log {
            file: Mutex::new(file),
        })
    })
    .as_ref()
}

pub fn enabled() -> bool {
    log().is_some()
}

pub fn count(part: &'static str) {
    if enabled() {
        COUNTS.with(|c| *c.borrow_mut().entry(part).or_default() += 1);
    }
}

thread_local! {
    static COUNTS: std::cell::RefCell<std::collections::BTreeMap<&'static str, u32>> =
        Default::default();
}

pub struct ShellProbe {
    child: Option<AnyElement>,
    render: Duration,
    started: Option<Instant>,
}

impl ShellProbe {
    pub fn new(child: AnyElement, render: Duration) -> Self {
        Self {
            child: Some(child),
            render,
            started: None,
        }
    }
}

impl IntoElement for ShellProbe {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for ShellProbe {
    type RequestLayoutState = AnyElement;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, AnyElement) {
        self.started = Some(Instant::now());
        let mut child = self.child.take().expect("probe child");
        let layout = child.request_layout(window, cx);
        let style = Style {
            size: size(relative(1.).into(), relative(1.).into()),
            ..Default::default()
        };
        (window.request_layout(style, [layout], cx), child)
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        child: &mut AnyElement,
        window: &mut Window,
        cx: &mut App,
    ) {
        child.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        child: &mut AnyElement,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        child.paint(window, cx);
        let Some(log) = log() else { return };
        let layout_paint = self.started.map_or(Duration::ZERO, |t| t.elapsed());
        let counts = COUNTS.with(|c| std::mem::take(&mut *c.borrow_mut()));
        let counts: Vec<String> = counts.iter().map(|(k, v)| format!("\"{k}\":{v}")).collect();
        if let Ok(mut file) = log.file.lock() {
            let _ = writeln!(
                file,
                "{{\"frame\":\"shell\",\"render_ms\":{:.4},\"layout_paint_ms\":{:.4},\"total_ms\":{:.4},\"parts\":{{{}}}}}",
                self.render.as_secs_f64() * 1000.,
                layout_paint.as_secs_f64() * 1000.,
                (self.render + layout_paint).as_secs_f64() * 1000.,
                counts.join(",")
            );
        }
    }
}
