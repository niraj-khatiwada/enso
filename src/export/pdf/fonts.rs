//! The fonts a PDF is set in: the system's own sans-serif and monospace
//! families, and fallbacks for scripts and symbols they lack. Found once per
//! run, and kept for it.

use krilla::text::Font;
use std::{ops::Range, sync::OnceLock};

/// A loaded face: what krilla embeds, and what measures text the way krilla
/// will draw it (both shape with rustybuzz).
pub struct Face {
    pub font: Font,
    shaper: rustybuzz::Face<'static>,
    /// Above and below the baseline, per point of font size.
    pub ascent: f32,
    pub descent: f32,
}

impl Face {
    fn load(data: &'static [u8], index: u32) -> Option<Self> {
        let shaper = rustybuzz::Face::from_slice(data, index)?;
        let font = Font::new(krilla::Data::from(data), index)?;
        let upem = shaper.units_per_em() as f32;
        Some(Self {
            font,
            ascent: shaper.ascender() as f32 / upem,
            descent: -(shaper.descender() as f32) / upem,
            shaper,
        })
    }

    pub fn has(&self, c: char) -> bool {
        self.shaper.glyph_index(c).is_some()
    }

    /// How wide `text` is at `size` points.
    pub fn width(&self, text: &str, size: f32) -> f32 {
        if text.is_empty() {
            return 0.;
        }
        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.guess_segment_properties();
        let output = rustybuzz::shape(&self.shaper, &[], buffer);
        let units: i32 = output.glyph_positions().iter().map(|p| p.x_advance).sum();
        units as f32 * size / self.shaper.units_per_em() as f32
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct Variant {
    pub bold: bool,
    pub italic: bool,
    pub mono: bool,
}

pub struct Fonts {
    /// Regular, bold, italic, bold italic.
    sans: [Face; 4],
    /// Regular, bold.
    mono: [Face; 2],
    /// Found up front, opened only when a note needs one: CJK fonts are big.
    fallback: Vec<(fontdb::ID, OnceLock<Option<Face>>)>,
    db: fontdb::Database,
}

const SANS: &[&str] = &[
    "Helvetica Neue",
    "Helvetica",
    "Segoe UI",
    "Inter",
    "Noto Sans",
    "DejaVu Sans",
    "Liberation Sans",
    "Arial",
];
const MONO: &[&str] = &[
    "Menlo",
    "SF Mono",
    "Consolas",
    "Cascadia Mono",
    "JetBrains Mono",
    "Noto Sans Mono",
    "DejaVu Sans Mono",
    "Liberation Mono",
    "Courier New",
];
/// Tried in order for characters the main families don't have.
const FALLBACK: &[&str] = &[
    "PingFang SC",
    "Hiragino Sans",
    "Apple SD Gothic Neo",
    "Microsoft YaHei",
    "Yu Gothic",
    "Malgun Gothic",
    "Noto Sans CJK SC",
    "Noto Sans CJK JP",
    "Noto Sans CJK KR",
    "Arial Unicode MS",
    "Apple Symbols",
    "Segoe UI Symbol",
    "Noto Sans Symbols",
    "Noto Sans Symbols 2",
    "DejaVu Sans",
];

impl Fonts {
    /// The fonts, found on first use.
    pub fn get() -> Result<&'static Fonts, String> {
        static FONTS: OnceLock<Result<Fonts, String>> = OnceLock::new();
        FONTS.get_or_init(Self::load).as_ref().map_err(Clone::clone)
    }

    fn load() -> Result<Self, String> {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        let face = |names: &[&str], generic: fontdb::Family<'static>, bold: bool, italic: bool| {
            let families: Vec<fontdb::Family> = names
                .iter()
                .map(|name| fontdb::Family::Name(name))
                .chain(std::iter::once(generic))
                .collect();
            let id = db.query(&fontdb::Query {
                families: &families,
                weight: if bold { fontdb::Weight::BOLD } else { fontdb::Weight::NORMAL },
                stretch: fontdb::Stretch::Normal,
                style: if italic { fontdb::Style::Italic } else { fontdb::Style::Normal },
            })?;
            load(&db, id)
        };
        let missing = || "No usable fonts were found on this system.".to_string();
        let sans = [
            face(SANS, fontdb::Family::SansSerif, false, false).ok_or_else(missing)?,
            face(SANS, fontdb::Family::SansSerif, true, false).ok_or_else(missing)?,
            face(SANS, fontdb::Family::SansSerif, false, true).ok_or_else(missing)?,
            face(SANS, fontdb::Family::SansSerif, true, true).ok_or_else(missing)?,
        ];
        let mono = [
            face(MONO, fontdb::Family::Monospace, false, false).ok_or_else(missing)?,
            face(MONO, fontdb::Family::Monospace, true, false).ok_or_else(missing)?,
        ];
        let fallback = FALLBACK
            .iter()
            .filter_map(|name| {
                db.query(&fontdb::Query {
                    families: &[fontdb::Family::Name(name)],
                    ..Default::default()
                })
            })
            .map(|id| (id, OnceLock::new()))
            .collect();
        Ok(Self { sans, mono, fallback, db })
    }

    pub fn face(&self, variant: Variant) -> &Face {
        match (variant.mono, variant.bold, variant.italic) {
            (true, bold, _) => &self.mono[bold as usize],
            (false, false, false) => &self.sans[0],
            (false, true, false) => &self.sans[1],
            (false, false, true) => &self.sans[2],
            (false, true, true) => &self.sans[3],
        }
    }

    /// `text` split into runs, each in the first face that has its
    /// characters: `primary` unless it lacks them.
    pub fn runs<'a>(&'a self, text: &str, primary: &'a Face) -> Vec<(&'a Face, Range<usize>)> {
        let mut runs: Vec<(&Face, Range<usize>)> = Vec::new();
        for (at, c) in text.char_indices() {
            let face = if c.is_whitespace() || c.is_control() || primary.has(c) {
                primary
            } else {
                self.fallback
                    .iter()
                    .filter_map(|(id, face)| face.get_or_init(|| load(&self.db, *id)).as_ref())
                    .find(|f| f.has(c))
                    .unwrap_or(primary)
            };
            let end = at + c.len_utf8();
            match runs.last_mut() {
                Some((last, range)) if std::ptr::eq(*last, face) => range.end = end,
                _ => runs.push((face, at..end)),
            }
        }
        runs
    }

    /// The width of `text` in `primary` (and its fallbacks) at `size`.
    pub fn width(&self, text: &str, primary: &Face, size: f32) -> f32 {
        self.runs(text, primary)
            .into_iter()
            .map(|(face, range)| face.width(&text[range], size))
            .sum()
    }
}

/// A face, its file mapped rather than read, kept for the rest of the run
/// (fonts are loaded once).
fn load(db: &fontdb::Database, id: fontdb::ID) -> Option<Face> {
    let (source, index) = db.face_source(id)?;
    let data: &'static [u8] = match source {
        fontdb::Source::File(path) => {
            let file = std::fs::File::open(path).ok()?;
            // Safety: system font files aren't changed while enso runs; if
            // one were, only a PDF's text would come out wrong.
            let map = unsafe { memmap2::Mmap::map(&file) }.ok()?;
            let map: &'static memmap2::Mmap = Box::leak(Box::new(map));
            &map[..]
        }
        fontdb::Source::SharedFile(_, data) => {
            let data: &[u8] = (*data).as_ref();
            Box::leak(data.to_vec().into_boxed_slice())
        }
        fontdb::Source::Binary(data) => {
            let data: &[u8] = (*data).as_ref();
            Box::leak(data.to_vec().into_boxed_slice())
        }
    };
    Face::load(data, index)
}
