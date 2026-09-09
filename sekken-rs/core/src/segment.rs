//! 大文字境界による入力の分割。

/// 入力を大文字境界で分割した結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segmented {
    /// 先頭の大文字より前にある、そのままかなにする部分。
    pub prefix: String,
    /// 大文字で始まる各セグメント。小文字に正規化済み。
    pub segments: Vec<String>,
}

/// 大文字を境界として入力を分割する。
pub fn segment(roman: &str) -> Segmented {
    let mut prefix = String::new();
    let mut segments: Vec<String> = Vec::new();
    for c in roman.chars() {
        if c.is_ascii_uppercase() {
            segments.push(c.to_ascii_lowercase().to_string());
        } else if let Some(last) = segments.last_mut() {
            last.push(c);
        } else {
            prefix.push(c);
        }
    }
    Segmented { prefix, segments }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(prefix: &str, segments: &[&str]) -> Segmented {
        Segmented {
            prefix: prefix.to_string(),
            segments: segments.iter().map(|s| s.to_string()).collect(),
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
    fn 先頭の小文字は_prefix_になる() {
        assert_eq!(
            segment("kyouHaIiTenki"),
            seg("kyou", &["ha", "ii", "tenki"])
        );
    }

    #[test]
    fn 大文字が無ければ全体が_prefix() {
        assert_eq!(segment("konnnichiha"), seg("konnnichiha", &[]));
    }
}
