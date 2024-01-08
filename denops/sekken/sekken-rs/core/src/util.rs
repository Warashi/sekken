pub fn is_hira(ch: char) -> bool {
    let code = ch as u32;
    matches!(code, 0x3040..=0x309F)
}

pub fn is_kata(ch: char) -> bool {
    let code = ch as u32;
    matches!(code, 0x30A0..=0x30FF)
}

pub fn is_kanji(ch: char) -> bool {
    let code = ch as u32;
    matches!(code, 0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0x20000..=0x2A6DF | 0x2A700..=0x2B73F | 0x2B740..=0x2B81F | 0x2B820..=0x2CEAF | 0xF900..=0xFAFF | 0x2F800..=0x2FA1F)
}

pub fn is_japanese_symbol(ch: char) -> bool {
    let code = ch as u32;
    matches!(code, 0x3000..=0x303F | 0x31F0..=0x31FF | 0x3220..=0x3243 | 0x3280..=0x337F | 0xFF5F..=0xFF9F)
}

pub fn is_japanese(ch: char) -> bool {
    is_hira(ch) || is_kata(ch) || is_kanji(ch) || is_japanese_symbol(ch)
}
