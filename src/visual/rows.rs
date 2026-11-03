use std::ops::Range;

use super::document::{Block, Kind, VisualDocument};

pub const CHUNK: usize = 24;

#[derive(Clone, Debug, PartialEq)]
pub struct RowRef {
    pub block: usize,
    pub part: Option<Range<usize>>,
}

#[derive(Default)]
pub struct Rows {
    pub rows: Vec<RowRef>,
    starts: Vec<usize>,
}

pub fn parts(block: &Block) -> Vec<Option<Range<usize>>> {
    let n = match &block.kind {
        Kind::List { items } => items.len(),
        Kind::Table { rows, .. } => rows.len(),
        _ => 0,
    };
    if n <= CHUNK + CHUNK / 2 {
        return vec![None];
    }
    (0..n)
        .step_by(CHUNK)
        .map(|start| Some(start..(start + CHUNK).min(n)))
        .collect()
}

impl Rows {
    pub fn build(doc: &VisualDocument) -> Self {
        let mut rows = Self::default();
        for (ix, block) in doc.blocks.iter().enumerate() {
            rows.starts.push(rows.rows.len());
            rows.rows.extend(
                parts(block)
                    .into_iter()
                    .map(|part| RowRef { block: ix, part }),
            );
        }
        rows.starts.push(rows.rows.len());
        rows
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn get(&self, ix: usize) -> Option<&RowRef> {
        self.rows.get(ix)
    }

    pub fn splice(
        &mut self,
        doc: &VisualDocument,
        old: Range<usize>,
        new_len: usize,
        inner: Option<Range<usize>>,
    ) -> (Range<usize>, usize) {
        let old_rows = self.starts[old.start]..self.starts[old.end];
        let new_parts: Vec<Vec<Option<Range<usize>>>> = doc.blocks[old.start..old.start + new_len]
            .iter()
            .map(parts)
            .collect();
        if old.len() == 1
            && new_len == 1
            && let Some(inner) = inner
        {
            let same = new_parts[0].len() == old_rows.len()
                && new_parts[0]
                    .iter()
                    .zip(&self.rows[old_rows.clone()])
                    .all(|(p, r)| *p == r.part);
            if same {
                let touched: Vec<usize> = old_rows
                    .clone()
                    .filter(|&r| {
                        self.rows[r].part.as_ref().is_none_or(|p| {
                            p.start < inner.end.max(inner.start + 1) && inner.start < p.end
                        })
                    })
                    .collect();
                if let (Some(&a), Some(&b)) = (touched.first(), touched.last()) {
                    return (a..b + 1, b + 1 - a);
                }
                return (old_rows.start..old_rows.start, 0);
            }
        }
        let new_rows: Vec<RowRef> = new_parts
            .into_iter()
            .enumerate()
            .flat_map(|(i, parts)| {
                parts.into_iter().map(move |part| RowRef {
                    block: old.start + i,
                    part,
                })
            })
            .collect();
        let count = new_rows.len();
        let delta = new_len as isize - old.len() as isize;
        self.rows.splice(old_rows.clone(), new_rows);
        for row in &mut self.rows[old_rows.start + count..] {
            row.block = (row.block as isize + delta) as usize;
        }
        self.reindex();
        (old_rows, count)
    }

    fn reindex(&mut self) {
        self.starts.clear();
        let mut block = 0;
        for (ix, row) in self.rows.iter().enumerate() {
            while block <= row.block {
                self.starts.push(ix);
                block += 1;
            }
        }
        while self.starts.len() <= block {
            self.starts.push(self.rows.len());
        }
        if let Some(last) = self.starts.last_mut() {
            *last = self.rows.len();
        }
    }

    pub fn row_of(&self, doc: &VisualDocument, bix: usize, lix: usize) -> usize {
        let first = self.starts.get(bix).copied().unwrap_or(0);
        let end = self.starts.get(bix + 1).copied().unwrap_or(self.rows.len());
        if end <= first + 1 {
            return first;
        }
        let units: Vec<Range<usize>> = match &doc.blocks[bix].kind {
            Kind::List { items } => items.iter().map(|i| i.leaves.clone()).collect(),
            Kind::Table { rows, .. } => rows.iter().map(|r| r.leaves.clone()).collect(),
            _ => return first,
        };
        let unit = units.iter().position(|u| u.contains(&lix)).unwrap_or(0);
        (first..end)
            .find(|&r| {
                self.rows[r]
                    .part
                    .as_ref()
                    .is_some_and(|p| p.contains(&unit))
            })
            .unwrap_or(first)
    }

    pub fn first_row(&self, bix: usize) -> usize {
        self.starts.get(bix).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::base::input::Rope;

    fn check(rows: &Rows, doc: &VisualDocument) {
        let fresh = Rows::build(doc);
        assert_eq!(rows.rows, fresh.rows);
        assert_eq!(rows.starts, fresh.starts);
    }

    #[test]
    fn long_lists_and_tables_split_and_follow_edits() {
        let list: String = (0..100).map(|i| format!("- item {i}\n")).collect();
        let table = "| a | b |\n|---|---|\n".to_string()
            + &(0..60)
                .map(|i| format!("| r{i} | x |\n"))
                .collect::<String>();
        let mut text = format!("# T\n\n{list}\npara\n\n{table}\nend\n");
        let mut doc = VisualDocument::parse(&text);
        let mut rows = Rows::build(&doc);
        assert_eq!(rows.len(), 1 + 5 + 1 + 3 + 1, "{:?}", rows.rows);
        let at = text.find("item 50").unwrap() + 5;
        text.insert(at, 'x');
        let r = doc.edit(&Rope::from(text.as_str()), at..at, 1);
        let (range, n) = rows.splice(&doc, r.splice.old, r.splice.new_len, r.inner);
        assert_eq!(range.len(), n);
        assert!(n <= 2, "{range:?}");
        check(&rows, &doc);
        text.insert_str(0, "intro\n\n");
        let r = doc.edit(&Rope::from(text.as_str()), 0..0, 7);
        rows.splice(&doc, r.splice.old, r.splice.new_len, r.inner);
        check(&rows, &doc);
    }
}
