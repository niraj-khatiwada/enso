#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    None,
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TableSource {
    pub header: Vec<String>,
    pub aligns: Vec<Align>,
    pub rows: Vec<Vec<String>>,
    pub crlf: bool,
}

pub fn split_cells(line: &str) -> Vec<String> {
    let line = line.trim();
    let line = line.strip_prefix('|').unwrap_or(line);
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut chars = line.chars().peekable();
    let mut ticks = 0usize;
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                cell.push(c);
                if let Some(next) = chars.next() {
                    cell.push(next);
                }
            }
            '`' => {
                let mut run = 1;
                while chars.peek() == Some(&'`') {
                    chars.next();
                    run += 1;
                }
                cell.extend(std::iter::repeat_n('`', run));
                ticks = if ticks == run {
                    0
                } else if ticks == 0 {
                    run
                } else {
                    ticks
                };
            }
            '|' if ticks == 0 => {
                cells.push(cell.trim().to_string());
                cell.clear();
            }
            _ => cell.push(c),
        }
    }
    if !cell.trim().is_empty() || !line.trim_end().ends_with('|') {
        cells.push(cell.trim().to_string());
    }
    cells
}

fn parse_align(cell: &str) -> Option<Align> {
    let cell = cell.trim();
    let left = cell.starts_with(':');
    let right = cell.ends_with(':');
    let dashes = cell.trim_matches(':');
    if dashes.is_empty() || !dashes.chars().all(|c| c == '-') {
        return None;
    }
    Some(match (left, right) {
        (true, true) => Align::Center,
        (true, false) => Align::Left,
        (false, true) => Align::Right,
        (false, false) => Align::None,
    })
}

pub fn parse(source: &str) -> Option<TableSource> {
    let crlf = source.contains("\r\n");
    let lines: Vec<&str> = source
        .lines()
        .map(|l| l.trim_end_matches('\r'))
        .filter(|l| !l.trim().is_empty())
        .collect();
    if lines.len() < 2 {
        return None;
    }
    let header = split_cells(lines[0]);
    let aligns: Vec<Align> = split_cells(lines[1])
        .iter()
        .map(|c| parse_align(c))
        .collect::<Option<_>>()?;
    let width = header.len().max(aligns.len());
    let pad = |mut row: Vec<String>| {
        row.resize(width, String::new());
        row
    };
    let mut aligns = aligns;
    aligns.resize(width, Align::None);
    Some(TableSource {
        header: pad(header),
        aligns,
        rows: lines[2..].iter().map(|l| pad(split_cells(l))).collect(),
        crlf,
    })
}

pub fn display_width(text: &str) -> usize {
    text.chars()
        .map(|c| {
            let c = c as u32;
            let wide = (0x1100..=0x115F).contains(&c)
                || (0x2E80..=0xA4CF).contains(&c)
                || (0xAC00..=0xD7A3).contains(&c)
                || (0xF900..=0xFAFF).contains(&c)
                || (0xFE30..=0xFE4F).contains(&c)
                || (0xFF00..=0xFF60).contains(&c)
                || (0xFFE0..=0xFFE6).contains(&c)
                || (0x1F300..=0x1FAFF).contains(&c);
            let zero = (0x0300..=0x036F).contains(&c)
                || (0x200B..=0x200F).contains(&c)
                || (0xFE00..=0xFE0F).contains(&c)
                || (0x1F3FB..=0x1F3FF).contains(&c)
                || (0x0900..=0x0903).contains(&c)
                || (0x093A..=0x094F).contains(&c)
                || c == 0x200D;
            if zero {
                0
            } else if wide {
                2
            } else {
                1
            }
        })
        .sum()
}

impl TableSource {
    pub fn columns(&self) -> usize {
        self.header.len()
    }

    pub fn format(&self) -> String {
        let columns = self.columns();
        let mut widths = vec![3usize; columns];
        for row in std::iter::once(&self.header).chain(&self.rows) {
            for (c, cell) in row.iter().enumerate().take(columns) {
                widths[c] = widths[c].max(display_width(cell));
            }
        }
        let line = |cells: &[String]| {
            let mut out = String::from("|");
            for (c, width) in widths.iter().enumerate() {
                let cell = cells.get(c).map(String::as_str).unwrap_or("");
                let pad = width - display_width(cell);
                let (before, after) = match self.aligns[c] {
                    Align::Right => (pad, 0),
                    Align::Center => (pad / 2, pad - pad / 2),
                    _ => (0, pad),
                };
                out.push(' ');
                out.push_str(&" ".repeat(before));
                out.push_str(cell);
                out.push_str(&" ".repeat(after));
                out.push_str(" |");
            }
            out
        };
        let mut lines = vec![line(&self.header)];
        let mut rule = String::from("|");
        for (c, width) in widths.iter().enumerate() {
            let dashes = "-".repeat(*width);
            rule.push_str(&match self.aligns[c] {
                Align::None => format!("-{dashes}-|"),
                Align::Left => format!(":{dashes}-|"),
                Align::Right => format!("-{dashes}:|"),
                Align::Center => format!(":{dashes}:|"),
            });
        }
        lines.push(rule);
        lines.extend(self.rows.iter().map(|row| line(row)));
        lines.join(if self.crlf { "\r\n" } else { "\n" })
    }

    pub fn insert_row(&mut self, at: usize) {
        let at = at.min(self.rows.len());
        self.rows.insert(at, vec![String::new(); self.columns()]);
    }

    pub fn delete_row(&mut self, at: usize) -> bool {
        if at < self.rows.len() {
            self.rows.remove(at);
            true
        } else {
            false
        }
    }

    pub fn insert_column(&mut self, at: usize) {
        let at = at.min(self.columns());
        self.header.insert(at, String::new());
        self.aligns.insert(at, Align::None);
        for row in &mut self.rows {
            row.insert(at, String::new());
        }
    }

    pub fn delete_column(&mut self, at: usize) -> bool {
        if self.columns() <= 1 || at >= self.columns() {
            return false;
        }
        self.header.remove(at);
        self.aligns.remove(at);
        for row in &mut self.rows {
            if at < row.len() {
                row.remove(at);
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cells_with_escapes_and_code() {
        assert_eq!(
            split_cells("| a \\| b | `x | y` | **c** |"),
            ["a \\| b", "`x | y`", "**c**"]
        );
        assert_eq!(split_cells("a | b"), ["a", "b"]);
        assert_eq!(split_cells("| a | |"), ["a", ""]);
    }

    #[test]
    fn formats_tidy_columns_and_edits_structure() {
        let source = "| Stage | km |\n|:--|--:|\n| Lake | 4.2 |\n| 中文 | 1 |";
        let mut table = parse(source).unwrap();
        assert_eq!(table.aligns, [Align::Left, Align::Right]);
        assert_eq!(
            table.format(),
            "| Stage |  km |\n|:------|----:|\n| Lake  | 4.2 |\n| 中文  |   1 |"
        );
        table.insert_row(1);
        table.insert_column(1);
        assert_eq!(table.rows[1], ["", "", ""]);
        assert!(table.delete_column(1));
        assert!(table.delete_row(1));
        assert_eq!(table.format(), parse(source).unwrap().format());
        let crlf = parse(&source.replace('\n', "\r\n")).unwrap();
        assert!(crlf.format().contains("\r\n"));
        let mut one = parse("| a |\n|---|").unwrap();
        assert!(!one.delete_column(0));
    }
}
