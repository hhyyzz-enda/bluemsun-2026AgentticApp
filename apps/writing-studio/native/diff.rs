//! Word-level diff for the review card. Chinese compares per character;
//! ASCII letter/digit runs compare as whole words. The review selections are
//! short, so a plain LCS dynamic program is right-sized; oversized inputs
//! fall back to a whole-passage replacement instead of a quadratic table.

/// How one token of the original fared in the proposal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffOp {
    Keep,
    Add,
    Del,
}

/// Splits text into comparable tokens: runs of ASCII letters/digits stay
/// whole words, everything else (each CJK character, punctuation, whitespace
/// runs) is its own token.
pub fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut word = String::new();
    macro_rules! flush {
        () => {
            if !word.is_empty() {
                tokens.push(std::mem::take(&mut word));
            }
        };
    }
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_ascii_alphanumeric() {
            word.push(c);
        } else {
            flush!();
            if c.is_whitespace() {
                let mut ws = String::from(c);
                while matches!(chars.peek(), Some(n) if n.is_whitespace()) {
                    ws.push(chars.next().unwrap());
                }
                tokens.push(ws);
            } else {
                tokens.push(c.to_string());
            }
        }
    }
    flush!();
    tokens
}

/// Above this token count the quadratic LCS table is not worth it: report the
/// whole passage as deleted and the whole proposal as added.
const MAX_TOKENS: usize = 400;

/// Longest-common-subsequence diff: kept tokens in order, deletions and
/// additions interleaved where they happened.
pub fn diff(before: &str, after: &str) -> Vec<(DiffOp, String)> {
    let a = tokenize(before);
    let b = tokenize(after);
    if a.len() > MAX_TOKENS || b.len() > MAX_TOKENS {
        return vec![
            (DiffOp::Del, before.to_owned()),
            (DiffOp::Add, after.to_owned()),
        ];
    }
    // lengths[i][j] = LCS length of a[i..] and b[j..].
    let (n, m) = (a.len(), b.len());
    let mut lengths = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lengths[i][j] = if a[i] == b[j] {
                lengths[i + 1][j + 1] + 1
            } else {
                lengths[i + 1][j].max(lengths[i][j + 1])
            };
        }
    }
    let mut ops = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if a[i] == b[j] {
            ops.push((DiffOp::Keep, a[i].clone()));
            i += 1;
            j += 1;
        } else if lengths[i + 1][j] >= lengths[i][j + 1] {
            ops.push((DiffOp::Del, a[i].clone()));
            i += 1;
        } else {
            ops.push((DiffOp::Add, b[j].clone()));
            j += 1;
        }
    }
    while i < n {
        ops.push((DiffOp::Del, a[i].clone()));
        i += 1;
    }
    while j < m {
        ops.push((DiffOp::Add, b[j].clone()));
        j += 1;
    }
    ops
}

/// Merges adjacent same-op tokens into runs for display.
pub fn runs(ops: Vec<(DiffOp, String)>) -> Vec<(DiffOp, String)> {
    let mut merged: Vec<(DiffOp, String)> = Vec::new();
    for (op, token) in ops {
        if let Some((last_op, last_text)) = merged.last_mut() {
            if *last_op == op {
                last_text.push_str(&token);
                continue;
            }
        }
        merged.push((op, token));
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn added(diff: &[(DiffOp, String)]) -> String {
        diff.iter().filter(|(op, _)| *op == DiffOp::Add).map(|(_, t)| t.as_str()).collect()
    }
    fn deleted(diff: &[(DiffOp, String)]) -> String {
        diff.iter().filter(|(op, _)| *op == DiffOp::Del).map(|(_, t)| t.as_str()).collect()
    }
    fn kept(diff: &[(DiffOp, String)]) -> String {
        diff.iter().filter(|(op, _)| *op == DiffOp::Keep).map(|(_, t)| t.as_str()).collect()
    }

    #[test]
    fn chinese_compares_per_character() {
        let d = diff("这是一个非常冗长的段落", "这是一个简洁的段落");
        assert_eq!(kept(&d), "这是一个的段落");
        assert_eq!(deleted(&d), "非常冗长");
        assert_eq!(added(&d), "简洁");
    }

    #[test]
    fn ascii_words_stay_whole() {
        let d = diff("the quick brown fox", "the quick red fox");
        // Both spaces around "brown" survive; only the word itself is deleted.
        assert_eq!(kept(&d), "the quick  fox");
        assert_eq!(deleted(&d), "brown");
        assert_eq!(added(&d), "red");
    }

    #[test]
    fn identical_text_is_all_kept() {
        let d = diff("完全相同[^1]", "完全相同[^1]");
        assert!(d.iter().all(|(op, _)| *op == DiffOp::Keep));
    }

    #[test]
    fn runs_merge_adjacent_tokens() {
        let merged = runs(diff("这是一个非常冗长的段落", "这是一个简洁的段落"));
        assert!(merged.iter().any(|(op, text)| *op == DiffOp::Del && text == "非常冗长"));
        assert!(merged.iter().any(|(op, text)| *op == DiffOp::Add && text == "简洁"));
    }
}
