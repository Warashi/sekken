//! 大文字境界による入力の分割。
//!
//! 境界は大文字と `;` のほか、abbrev 区間を開く `/` と、接頭辞・接尾辞の印になる `>`。
//! エディタ側（lisp/sekken-input.el）も同じ規則で分けるので、規則を変えるときは両方を直す。

/// 入力を境界で分割した結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segmented {
    /// 先頭の境界より前にある、そのままかなにする部分。
    pub head: String,
    /// 境界で始まる各区間。
    pub segments: Vec<Segment>,
}

/// 境界で始まる 1 区間。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Segment {
    /// 小文字に正規化したローマ字。abbrev なら打った綴りそのまま。
    pub text: String,
    /// `/` で開いた区間。かなにせず、綴りを abbrev の見出しとして引く。
    pub abbrev: bool,
    /// 区間の直後に `>` がある。接頭辞の見出し（`お>`）でも引く。
    pub prefix: bool,
    /// 区間の直前に `>` がある。接尾辞の見出し（`>かい`）でも引く。
    pub suffix: bool,
}

impl Segment {
    fn convert(text: impl Into<String>) -> Segment {
        Segment {
            text: text.into(),
            ..Segment::default()
        }
    }
}

/// 境界で入力を分割する。
///
/// `;` `/` `>` で開いた直後の区間に大文字や `;` が続いても、新しい区間は作らず
/// その区間を続ける（`O>Kai` や `;o>;kai` の `かい` が `>` の印を受け取るため）。
/// 開いた直後の `/` はその区間を abbrev にする。
/// abbrev 区間は次の `;` か `/` まで続き、中の大文字は境界にしない。
pub fn segment(roman: &str) -> Segmented {
    let mut head = String::new();
    let mut segments: Vec<Segment> = Vec::new();
    let mut chars = roman.chars().peekable();
    // 境界で開いたばかりで、まだ文字の無い区間か。
    let mut fresh = false;
    // `/` で開いた abbrev 区間の中か。
    let mut in_abbrev = false;
    while let Some(c) = chars.next() {
        if in_abbrev {
            if c == ';' || c == '/' {
                in_abbrev = false;
                segments.push(Segment::convert(""));
                fresh = true;
            } else if let Some(last) = segments.last_mut() {
                last.text.push(c);
            }
            continue;
        }
        if c.is_ascii_uppercase() {
            if !fresh {
                segments.push(Segment::convert(""));
            }
            segments
                .last_mut()
                .unwrap()
                .text
                .push(c.to_ascii_lowercase());
            fresh = false;
        } else if c == ';' && chars.peek() == Some(&';') {
            chars.next();
            if let Some(last) = segments.last_mut() {
                last.text.push(';');
            } else {
                head.push(';');
            }
            fresh = false;
        } else if c == ';' {
            if !fresh {
                segments.push(Segment::convert(""));
            }
            fresh = true;
        } else if c == '/' {
            if !fresh {
                segments.push(Segment::default());
            }
            // abbrev は綴りで引くので `>` の印は持たない。
            *segments.last_mut().unwrap() = Segment {
                abbrev: true,
                ..Segment::default()
            };
            in_abbrev = true;
            fresh = false;
        } else if c == '>' {
            if let Some(last) = segments.last_mut() {
                last.prefix = true;
            }
            segments.push(Segment {
                suffix: true,
                ..Segment::default()
            });
            fresh = true;
        } else if let Some(last) = segments.last_mut() {
            last.text.push(c);
            fresh = false;
        } else {
            head.push(c);
        }
    }
    Segmented { head, segments }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(head: &str, segments: &[&str]) -> Segmented {
        Segmented {
            head: head.to_string(),
            segments: segments.iter().map(|s| Segment::convert(*s)).collect(),
        }
    }

    #[test]
    fn 大文字ごとに区切る() {
        assert_eq!(
            segment("WagahaiHaNekoDearu."),
            seg("", &["wagahai", "ha", "neko", "dearu."])
        );
    }

    #[test]
    fn 先頭の小文字は_head_になる() {
        assert_eq!(
            segment("kyouHaIiTenki"),
            seg("kyou", &["ha", "ii", "tenki"])
        );
    }

    #[test]
    fn 大文字が無ければ全体が_head() {
        assert_eq!(segment("konnnichiha"), seg("konnnichiha", &[]));
    }

    #[test]
    fn セミコロンを大文字と同じ境界として扱う() {
        assert_eq!(segment(";shokai;kougi"), seg("", &["shokai", "kougi"]));
        assert_eq!(segment("Neko;"), seg("", &["neko", ""]));
    }

    #[test]
    fn 二重セミコロンはリテラルのセミコロンになる() {
        assert_eq!(segment("semi;;koron"), seg("semi;koron", &[]));
        assert_eq!(segment(";semi;;koron"), seg("", &["semi;koron"]));
    }

    #[test]
    fn 境界の直後の大文字やセミコロンは新しい区間を作らない() {
        assert_eq!(segment(";Kai"), seg("", &["kai"]));
        assert_eq!(segment("Neko;Kai"), seg("", &["neko", "kai"]));
        assert_eq!(segment("Neko;;;kai"), seg("", &["neko;", "kai"]));
        assert_eq!(segment(";o>;kai"), segment("O>Kai"));
        assert_eq!(segment("Neko;"), seg("", &["neko", ""]));
    }

    #[test]
    fn 境界の直後のスラッシュはその区間を_abbrev_にする() {
        assert_eq!(
            segment("O>/emacs").segments,
            [
                Segment {
                    text: "o".to_string(),
                    prefix: true,
                    ..Segment::default()
                },
                Segment {
                    text: "emacs".to_string(),
                    abbrev: true,
                    ..Segment::default()
                },
            ]
        );
        assert_eq!(segment(";/emacs").segments.len(), 1);
    }

    #[test]
    fn スラッシュで開いた区間は綴りのまま_abbrev_になる() {
        let abbrev = |text: &str| Segment {
            text: text.to_string(),
            abbrev: true,
            ..Segment::default()
        };
        assert_eq!(
            segment("/emacs"),
            Segmented {
                head: String::new(),
                segments: vec![abbrev("emacs")]
            }
        );
        // 中の大文字は境界でなく綴りの一部。
        assert_eq!(segment("/GPL").segments, [abbrev("GPL")]);
        // `;` か `/` で閉じ、続きは変換区間になる。
        assert_eq!(
            segment("/emacs;ha").segments,
            [abbrev("emacs"), Segment::convert("ha")]
        );
        assert_eq!(
            segment("/emacs/Ha").segments,
            [abbrev("emacs"), Segment::convert("ha")]
        );
        assert_eq!(
            segment("Kyou/emacs").segments,
            [Segment::convert("kyou"), abbrev("emacs")]
        );
    }

    #[test]
    fn 山括弧は前の区間に接頭辞の印を後の区間に接尾辞の印を付ける() {
        assert_eq!(
            segment("O>Kai").segments,
            [
                Segment {
                    text: "o".to_string(),
                    prefix: true,
                    ..Segment::default()
                },
                Segment {
                    text: "kai".to_string(),
                    suffix: true,
                    ..Segment::default()
                },
            ]
        );
        assert_eq!(segment("Toukyou>kai").segments[1].text, "kai");
        assert!(segment("Toukyou>kai").segments[1].suffix);
        // 両側に `>` があれば両方の印を持つ。
        let both = &segment("O>Kai>Sha").segments[1];
        assert!(both.prefix && both.suffix);
    }

    #[test]
    fn 先頭の小文字の後の山括弧は接尾辞の区間だけを作る() {
        assert_eq!(
            segment("o>kai"),
            Segmented {
                head: "o".to_string(),
                segments: vec![Segment {
                    text: "kai".to_string(),
                    suffix: true,
                    ..Segment::default()
                }]
            }
        );
    }
}
