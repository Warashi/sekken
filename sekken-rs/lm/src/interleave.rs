//! 読みと出力を区間ごとに交互に並べる形。
//!
//! 「読み全体 \t 出力全体」だと読みが 1 字伸びるたびに出力側の文脈が全部変わり、
//! 前の変換で読んだ状態を出力側で使い回せない。区間ごとに交互に並べれば、
//! 先頭の区間の列は後ろに何を足しても変わらないので、読み終えた状態を
//! 次の変換に持ち越せる。
//!
//! 区間は漢字の並びの先頭で切り、後ろに続くかなは同じ区間に含める
//! （今日も | 晴天なり。）。送り仮名や助詞を漢字と同じ区間に置くと、
//! 漢字を出す位置でその読み（ヨム → 読む）を見られる。区間の分け方は
//! 表層と読みの文字列だけから決まるので、学習データを作る側と採点する側が
//! 同じ規則で揃う。

use sekken_core::kana::hira2kata_char;

/// 区間の読みの前に置く字。
pub const READING: char = '\u{1e}';
/// 区間の出力の前に置く字。
pub const OUTPUT: char = '\t';

/// 表層を「変換される並び」と「読みにそのまま現れる並び」に分けたときの 1 並び。
struct Run {
    converted: bool,
    chars: Vec<char>,
}

/// 読みにそのまま現れず、変換で決まる字か。漢字と、漢字扱いの記号・小書きのカ・ケ。
fn is_converted(c: char) -> bool {
    matches!(
        c as u32,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x3005 | 0x3006 | 0x3007 | 0x30F5 | 0x30F6
    )
}

fn runs(surface: &str) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    for c in surface.chars() {
        let converted = is_converted(c);
        match runs.last_mut() {
            Some(run) if run.converted == converted => run.chars.push(c),
            _ => runs.push(Run {
                converted,
                chars: vec![c],
            }),
        }
    }
    runs
}

/// `runs[i..]` を `reading[pos..]` に当て、各並びが消費した読みの長さを返す。
/// そのまま現れる並びは字ごとに一致させ、変換される並びは短い方から試す。
fn matches(runs: &[Run], reading: &[char], pos: usize) -> Option<Vec<usize>> {
    let Some(run) = runs.first() else {
        return (pos == reading.len()).then(Vec::new);
    };
    if !run.converted {
        let n = run.chars.len();
        if pos + n > reading.len() {
            return None;
        }
        let literal = run
            .chars
            .iter()
            .zip(&reading[pos..pos + n])
            .all(|(&s, &r)| hira2kata_char(s) == r);
        if !literal {
            return None;
        }
        let mut rest = matches(&runs[1..], reading, pos + n)?;
        rest.insert(0, n);
        return Some(rest);
    }
    for n in 1..=(reading.len() - pos) {
        if let Some(mut rest) = matches(&runs[1..], reading, pos + n) {
            rest.insert(0, n);
            return Some(rest);
        }
    }
    None
}

/// 表層 `surface` と読み `reading`（カタカナ）を区間に分け、(読み, 表層) の列を返す。
/// 読みが表層に当てはまらなければ `None`。
pub fn align(surface: &str, reading: &str) -> Option<Vec<(String, String)>> {
    let runs = runs(surface);
    let reading: Vec<char> = reading.chars().collect();
    if runs.is_empty() || reading.is_empty() {
        return None;
    }
    let lengths = matches(&runs, &reading, 0)?;
    let mut segments: Vec<(String, String)> = Vec::new();
    let mut pos = 0;
    for (run, n) in runs.iter().zip(lengths) {
        let read: String = reading[pos..pos + n].iter().collect();
        let text: String = run.chars.iter().collect();
        pos += n;
        // 並びは交互に現れるので、そのまま現れる並びの前は変換される並びか文頭。
        // 変換される並びの後ろのかなは同じ区間に含め、文頭のかなは独立の区間にする。
        match segments.last_mut() {
            Some((r, s)) if !run.converted => {
                r.push_str(&read);
                s.push_str(&text);
            }
            _ => segments.push((read, text)),
        }
    }
    Some(segments)
}

/// 学習・採点に使う 1 行。区間ごとに `READING` 読み `OUTPUT` 出力を並べる。
/// 読みが表層に当てはまらなければ全体を 1 区間にする。
pub fn build(reading: &str, output: &str) -> String {
    let segments =
        align(output, reading).unwrap_or_else(|| vec![(reading.to_string(), output.to_string())]);
    let mut line = String::new();
    for (read, text) in segments {
        line.push(READING);
        line.push_str(&read);
        line.push(OUTPUT);
        line.push_str(&text);
    }
    line
}

/// `build` が返す行を 1 区間だけで作ったか（当てはまらなかった行）。
pub fn is_fallback(line: &str) -> bool {
    line.chars().filter(|&c| c == READING).count() == 1
}

/// 行の各字が損失（採点）に入るか。出力の字と、区間の終わりを示す `READING` は
/// 入り、読みの字と `OUTPUT` は入らない。`READING` の無い行は `OUTPUT` の後ろ
/// だけ、どちらも無い行は全部が入る。
pub fn loss_mask(line: &str) -> Vec<bool> {
    let chars: Vec<char> = line.chars().collect();
    if !chars.contains(&READING) {
        let start = chars.iter().position(|&c| c == OUTPUT).map_or(0, |i| i + 1);
        return (0..chars.len()).map(|i| i >= start).collect();
    }
    let mut in_output = false;
    chars
        .iter()
        .map(|&c| match c {
            READING => {
                in_output = false;
                true
            }
            OUTPUT => {
                in_output = true;
                false
            }
            _ => in_output,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aligned(surface: &str, reading: &str) -> Vec<(String, String)> {
        align(surface, reading).unwrap()
    }

    fn pairs(v: &[(String, String)]) -> Vec<(&str, &str)> {
        v.iter().map(|(r, s)| (r.as_str(), s.as_str())).collect()
    }

    #[test]
    fn 漢字の先頭で切り後ろのかなを含める() {
        let v = aligned("今日も晴天なり。", "キョウモセイテンナリ。");
        assert_eq!(
            pairs(&v),
            [("キョウモ", "今日も"), ("セイテンナリ。", "晴天なり。")]
        );
    }

    #[test]
    fn 文頭のかなは独立の区間になる() {
        let v = aligned("それは猫だ", "ソレハネコダ");
        assert_eq!(pairs(&v), [("ソレハ", "それは"), ("ネコダ", "猫だ")]);
    }

    #[test]
    fn 送り仮名は漢字と同じ区間に入る() {
        let v = aligned("本を読む", "ホンヲヨム");
        assert_eq!(pairs(&v), [("ホンヲ", "本を"), ("ヨム", "読む")]);
    }

    #[test]
    fn 途中にかなを挟む語は漢字ごとに割れる() {
        let v = aligned("取り扱い", "トリアツカイ");
        assert_eq!(pairs(&v), [("トリ", "取り"), ("アツカイ", "扱い")]);
    }

    #[test]
    fn 同じかなが繰り返されても順に当てる() {
        let v = aligned("生き生き", "イキイキ");
        assert_eq!(pairs(&v), [("イキ", "生き"), ("イキ", "生き")]);
    }

    #[test]
    fn 末尾のかなは末尾に当てる() {
        // 「キ」は 1 字目にもあるが、末尾の「き」は読みの末尾に当てる。
        let v = aligned("気付き", "キヅキ");
        assert_eq!(pairs(&v), [("キヅキ", "気付き")]);
    }

    #[test]
    fn カタカナと記号は読みにそのまま現れる() {
        let v = aligned("猫、ネコ。", "ネコ、ネコ。");
        assert_eq!(pairs(&v), [("ネコ、ネコ。", "猫、ネコ。")]);
    }

    #[test]
    fn 小書きのケは変換される字として扱う() {
        let v = aligned("霞ヶ関へ", "カスミガセキヘ");
        assert_eq!(pairs(&v), [("カスミガセキヘ", "霞ヶ関へ")]);
    }

    #[test]
    fn 読みが当てはまらなければ_none() {
        assert!(align("猫が鳴く", "ネコガ").is_none());
        assert!(align("猫が鳴く", "ネコノナク").is_none());
        assert!(align("猫", "").is_none());
        assert!(align("", "ネコ").is_none());
    }

    #[test]
    fn 行は区間ごとに読みと出力を交互に並べる() {
        assert_eq!(
            build("ソレハネコダ", "それは猫だ"),
            "\u{1e}ソレハ\tそれは\u{1e}ネコダ\t猫だ"
        );
    }

    #[test]
    fn 当てはまらない行は全体を_1_区間にする() {
        let line = build("ネコノナク", "猫が鳴く");
        assert_eq!(line, "\u{1e}ネコノナク\t猫が鳴く");
        assert!(is_fallback(&line));
        assert!(!is_fallback(&build("ソレハネコダ", "それは猫だ")));
    }

    #[test]
    fn 損失は出力の字と区間の終わりに掛かる() {
        let line = "\u{1e}ネコ\t猫\u{1e}ガ\tが";
        assert_eq!(
            loss_mask(line),
            [true, false, false, false, true, true, false, false, true]
        );
    }

    #[test]
    fn 交互でない行はタブの後ろだけ_タブも無ければ全部() {
        assert_eq!(loss_mask("ネコ\t猫"), [false, false, false, true]);
        assert_eq!(loss_mask("猫が"), [true, true]);
    }
}
