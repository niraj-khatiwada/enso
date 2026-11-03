use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Sizable as _,
    button::{Button, ButtonVariants as _},
};
use gpui_kit::*;

use super::document::Kind;
use super::table_edit;
use super::view::{PAGE_TOP, VisualView};
use crate::tokens;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableOp {
    RowAbove,
    RowBelow,
    ColumnLeft,
    ColumnRight,
    DeleteRow,
    DeleteColumn,
    Tidy,
}

impl VisualView {
    pub fn table_cell(&self) -> Option<(usize, usize, usize)> {
        let (bix, lix, _) = self.doc.locate(self.caret)?;
        let Kind::Table { rows, .. } = &self.doc.blocks[bix].kind else {
            return None;
        };
        let r = rows.iter().position(|row| row.leaves.contains(&lix))?;
        Some((bix, r, lix - rows[r].leaves.start))
    }

    fn place_in_cell(
        &mut self,
        start: usize,
        row: usize,
        col: usize,
        end: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(bix) = self
            .doc
            .blocks
            .iter()
            .position(|b| b.start <= start && start < b.end.max(b.start + 1))
        else {
            return;
        };
        let Kind::Table { rows, .. } = &self.doc.blocks[bix].kind else {
            return;
        };
        let Some(r) = rows.get(row.min(rows.len().saturating_sub(1))) else {
            return;
        };
        let count = r.leaves.len();
        if count == 0 {
            return;
        }
        let lix = r.leaves.start + col.min(count - 1);
        let len = self.doc.blocks[bix].leaves[lix].len();
        let caret = self.doc.src_of(bix, lix, if end { len } else { 0 });
        self.place_caret(caret, None, cx);
    }

    pub fn table_op(&mut self, op: TableOp, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some((bix, row, col)) = self.table_cell() else {
            return false;
        };
        let block = &self.doc.blocks[bix];
        let range = block.start..block.end;
        let source = self.editor.read(cx).text().slice(range.clone()).to_string();
        let trailing = source.len() - source.trim_end_matches(['\n', '\r']).len();
        let Some(mut table) = table_edit::parse(&source) else {
            return false;
        };
        let body = row.checked_sub(1);
        let (row, col) = match op {
            TableOp::RowAbove => {
                let at = body.unwrap_or(0);
                table.insert_row(at);
                (at + 1, col)
            }
            TableOp::RowBelow => {
                let at = body.map_or(0, |b| b + 1);
                table.insert_row(at);
                (at + 1, col)
            }
            TableOp::ColumnLeft => {
                table.insert_column(col);
                (row, col)
            }
            TableOp::ColumnRight => {
                table.insert_column(col + 1);
                (row, col + 1)
            }
            TableOp::DeleteRow => {
                let Some(b) = body else {
                    return true;
                };
                table.delete_row(b);
                (row.min(table.rows.len()), col)
            }
            TableOp::DeleteColumn => {
                if !table.delete_column(col) {
                    return true;
                }
                (row, col.min(table.columns() - 1))
            }
            TableOp::Tidy => (row, col),
        };
        let mut text = table.format();
        text.push_str(&source[source.len() - trailing..]);
        if text != source {
            self.edit(range.clone(), &text, window, cx);
        }
        self.place_in_cell(range.start, row, col, false, cx);
        true
    }

    pub fn next_cell(&mut self, back: bool, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some((bix, row, col)) = self.table_cell() else {
            return false;
        };
        let Kind::Table { rows, .. } = &self.doc.blocks[bix].kind else {
            return false;
        };
        let start = self.doc.blocks[bix].start;
        let columns = rows[row].leaves.len();
        let count = rows.len();
        let target = if back {
            if col > 0 {
                Some((row, col - 1))
            } else if row > 0 {
                Some((row - 1, rows[row - 1].leaves.len().saturating_sub(1)))
            } else {
                None
            }
        } else if col + 1 < columns {
            Some((row, col + 1))
        } else if row + 1 < count {
            Some((row + 1, 0))
        } else {
            None
        };
        match target {
            Some((row, col)) => self.place_in_cell(start, row, col, true, cx),
            None if !back => {
                self.table_op(TableOp::RowBelow, window, cx);
                if let Some((bix, row, _)) = self.table_cell() {
                    let start = self.doc.blocks[bix].start;
                    self.place_in_cell(start, row, 0, false, cx);
                }
            }
            None => {}
        }
        true
    }

    pub fn render_table_toolbar(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.focus.is_focused(window) || self.image_dialog.is_some() {
            return None;
        }
        let (bix, _, _) = self.table_cell()?;
        let lix = match &self.doc.blocks[bix].kind {
            Kind::Table { rows, .. } => rows.first()?.leaves.start,
            _ => return None,
        };
        let first_row = self.rows.row_of(&self.doc, bix, lix);
        let viewport = self.list.viewport_bounds();
        let table_top = self
            .list
            .bounds_for_item(first_row)
            .map(|b| b.top())
            .unwrap_or(viewport.top());
        let top = (table_top - viewport.top()).max(px(0.)) + px(PAGE_TOP) - tokens::FLOAT_OFFSET;
        let theme = cx.theme().clone();
        let button = |id: &'static str, icon: IconName, tip: &'static str, op: TableOp| {
            Button::new(id)
                .ghost()
                .xsmall()
                .icon(icon)
                .tooltip(tip)
                .accessibility_label(tip)
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.table_op(op, window, cx);
                    window.focus(&this.focus, cx);
                }))
        };
        Some(
            div()
                .absolute()
                .top(top.max(tokens::SPACE_1))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(
                    div()
                        .w(tokens::PAGE_WIDTH)
                        .max_w_full()
                        .px_4()
                        .flex()
                        .justify_end()
                        .child(
                            div()
                                .id("table-toolbar")
                                .flex()
                                .items_center()
                                .gap_0p5()
                                .p_0p5()
                                .rounded_md()
                                .bg(theme.popover)
                                .border_1()
                                .border_color(theme.border)
                                .shadow_md()
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .child(button(
                                    "table-row-above",
                                    IconName::BetweenHorizontalStart,
                                    "Insert row above",
                                    TableOp::RowAbove,
                                ))
                                .child(button(
                                    "table-row-below",
                                    IconName::BetweenHorizontalEnd,
                                    "Insert row below",
                                    TableOp::RowBelow,
                                ))
                                .child(button(
                                    "table-col-left",
                                    IconName::BetweenVerticalStart,
                                    "Insert column left",
                                    TableOp::ColumnLeft,
                                ))
                                .child(button(
                                    "table-col-right",
                                    IconName::BetweenVerticalEnd,
                                    "Insert column right",
                                    TableOp::ColumnRight,
                                ))
                                .child(
                                    div()
                                        .w(tokens::HAIRLINE)
                                        .h(tokens::ICON)
                                        .mx_0p5()
                                        .bg(theme.border),
                                )
                                .child(button(
                                    "table-del-row",
                                    IconName::TableRowsSplit,
                                    "Delete row",
                                    TableOp::DeleteRow,
                                ))
                                .child(button(
                                    "table-del-col",
                                    IconName::TableColumnsSplit,
                                    "Delete column",
                                    TableOp::DeleteColumn,
                                ))
                                .child(
                                    div()
                                        .w(tokens::HAIRLINE)
                                        .h(tokens::ICON)
                                        .mx_0p5()
                                        .bg(theme.border),
                                )
                                .child(button(
                                    "table-tidy",
                                    IconName::Sparkles,
                                    "Tidy column padding",
                                    TableOp::Tidy,
                                )),
                        ),
                )
                .into_any_element(),
        )
    }
}
