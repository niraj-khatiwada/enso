use std::borrow::Cow;

use gpui_kit::*;

pub const EXCALIFONT_FAMILY: &str = "Excalifont";

pub const VIRGIL_FAMILY: &str = "Virgil";

pub const EXCALIFONT_TTF: &[u8] = include_bytes!("../../assets/fonts/Excalifont-Regular.ttf");

pub const VIRGIL_TTF: &[u8] = include_bytes!("../../assets/fonts/Virgil-Regular.ttf");

pub fn hand_drawn_fonts() -> Vec<Cow<'static, [u8]>> {
    vec![Cow::Borrowed(EXCALIFONT_TTF), Cow::Borrowed(VIRGIL_TTF)]
}

pub fn register_hand_drawn_fonts(cx: &App) {
    let _ = cx.text_system().add_fonts(hand_drawn_fonts());
}

#[cfg(test)]
mod tests {

    use super::{EXCALIFONT_FAMILY, EXCALIFONT_TTF, VIRGIL_FAMILY, VIRGIL_TTF, hand_drawn_fonts};

    fn is_truetype(bytes: &[u8]) -> bool {
        bytes.len() > 12 && bytes[0..4] == [0x00, 0x01, 0x00, 0x00]
    }

    #[test]
    fn embedded_faces_are_truetype_not_woff2() {
        for font in hand_drawn_fonts() {
            assert!(
                is_truetype(&font),
                "embedded font is not a TrueType sfnt container",
            );
        }
    }

    #[test]
    fn both_hand_drawn_families_are_embedded() {
        let fonts = hand_drawn_fonts();
        assert_eq!(fonts.len(), 2);
        assert!(fonts.iter().any(|f| f.len() == EXCALIFONT_TTF.len()));
        assert!(fonts.iter().any(|f| f.len() == VIRGIL_TTF.len()));
    }

    fn be_u16(b: &[u8], at: usize) -> u16 {
        u16::from_be_bytes([b[at], b[at + 1]])
    }

    fn be_u32(b: &[u8], at: usize) -> u32 {
        u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
    }

    fn declared_family_names(bytes: &[u8]) -> Vec<String> {
        let num_tables = be_u16(bytes, 4) as usize;
        let name_table = (0..num_tables)
            .map(|i| 12 + i * 16)
            .find(|rec| &bytes[*rec..*rec + 4] == b"name")
            .map(|rec| be_u32(bytes, rec + 8) as usize)
            .expect("face has no name table");
        let count = be_u16(bytes, name_table + 2) as usize;
        let storage = name_table + be_u16(bytes, name_table + 4) as usize;
        let mut names = Vec::new();
        for i in 0..count {
            let rec = name_table + 6 + i * 12;

            if be_u16(bytes, rec) != 3 || be_u16(bytes, rec + 2) != 1 {
                continue;
            }
            let name_id = be_u16(bytes, rec + 6);
            if name_id != 1 && name_id != 16 {
                continue;
            }
            let len = be_u16(bytes, rec + 8) as usize;
            let off = storage + be_u16(bytes, rec + 10) as usize;
            let units: Vec<u16> = bytes[off..off + len]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_be_bytes([c[0], c[1]]))
                .collect();
            names.push(String::from_utf16_lossy(&units));
        }
        names
    }

    #[test]
    fn embedded_faces_declare_the_family_the_canvas_asks_for() {
        let names = declared_family_names(EXCALIFONT_TTF);
        assert!(
            names.iter().any(|n| n == EXCALIFONT_FAMILY),
            "Excalifont face declares {names:?}",
        );
        let names = declared_family_names(VIRGIL_TTF);
        assert!(
            names.iter().any(|n| n == VIRGIL_FAMILY),
            "Virgil face declares {names:?}",
        );
    }
}
