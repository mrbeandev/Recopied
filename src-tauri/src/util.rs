/// Truncate `s` to at most `max_bytes` bytes without splitting a UTF-8 character.
///
/// Slicing a `&str` with `&s[..n]` panics when `n` falls inside a multi-byte
/// character (emoji, accented letters, Indic scripts, "—", ...). That panic used
/// to kill the clipboard watcher thread, which silently stopped history recording.
pub fn truncate_at_char_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// First 12 hex chars of a content hash — enough to correlate events without
/// storing anything that identifies the clipboard contents.
pub fn short_hash(hash: &str) -> &str {
    truncate_at_char_boundary(hash, 12)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_is_cut_exactly() {
        assert_eq!(truncate_at_char_boundary("abcdef", 3), "abc");
        assert_eq!(truncate_at_char_boundary("abc", 10), "abc");
    }

    #[test]
    fn never_splits_multibyte_chars() {
        // 199 ASCII bytes followed by a 3-byte "—": byte 200 is mid-character.
        let s = format!("{}—tail", "a".repeat(199));
        let t = truncate_at_char_boundary(&s, 200);
        assert_eq!(t.len(), 199);

        let telugu = "తెలుగు".repeat(50);
        for n in 0..telugu.len() {
            let t = truncate_at_char_boundary(&telugu, n);
            assert!(t.len() <= n);
        }

        let emoji = "😀".repeat(100);
        assert_eq!(truncate_at_char_boundary(&emoji, 201).len(), 200);
    }
}
