//! The element under the live view: it lays the page out while gpui-kit
//! reports where every line of text sits (and paints the selection over
//! them), then paints the caret and hands the platform's text input
//! (typing, IME) to the view.

use super::LiveView;
use gpui_kit::base::text::editor_host::{LeafGeometry, collect_leaf_geometry};
use gpui_kit::component::ActiveTheme;
use gpui_kit::*;
use std::{cell::RefCell, collections::HashMap, ops::Range, rc::Rc};

/// The key of a new, empty paragraph being written between blocks.
pub const GAP_LEAF: u64 = 1;

/// The key line `index` of the page reports its layout under.
pub fn leaf_key(index: usize) -> u64 {
    100 + index as u64
}

pub struct LiveElement {
    view: Entity<LiveView>,
    geometry: Rc<RefCell<HashMap<u64, LeafGeometry>>>,
    /// The selection to paint, per line: drawn ranges and their colour.
    decorations: Option<HashMap<u64, Vec<(Range<usize>, Hsla)>>>,
    child: Option<AnyElement>,
}

impl LiveElement {
    pub fn new(
        view: Entity<LiveView>,
        geometry: Rc<RefCell<HashMap<u64, LeafGeometry>>>,
        decorations: HashMap<u64, Vec<(Range<usize>, Hsla)>>,
        child: AnyElement,
    ) -> Self {
        Self { view, geometry, decorations: Some(decorations), child: Some(child) }
    }
}

impl IntoElement for LiveElement {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for LiveElement {
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
        let mut child = self.child.take().unwrap_or_else(|| div().into_any_element());
        (child.request_layout(window, cx), child)
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
        let decorations = self.decorations.take().unwrap_or_default();
        let (_, leaves) = collect_leaf_geometry(decorations, || child.prepaint(window, cx));
        *self.geometry.borrow_mut() = leaves;
        self.view.update(cx, |view, cx| view.finish_reveal(cx));
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        child: &mut AnyElement,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus = self.view.read(cx).focus.clone();
        window.handle_input(&focus, ElementInputHandler::new(bounds, self.view.clone()), cx);
        child.paint(window, cx);
        if !focus.is_focused(window) {
            return;
        }
        let view = self.view.read(cx);
        // The caret, when nothing is selected.
        if view.has_selection() {
            return;
        }
        let (Some(key), Some(local)) = (view.active_key(), view.caret_in_active()) else { return };
        if let Some(geometry) = self.geometry.borrow().get(&key) {
            window.paint_quad(fill(super::caret_bounds(geometry, local), cx.theme().caret));
        }
    }
}
