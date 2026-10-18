//! Fuzzy matching for quick open and the command palette (R-COL-9, R-COL-10).

/// How well `text` matches `query`, higher is better; `None` if it doesn't match.
///
/// Every word of the query must be found, in any order, case-insensitively: as a
/// substring (best at the start of a word), or else as letters in order (`gtusr`
/// finds "Get user").
pub fn score(query: &str, text: &str) -> Option<i64> {
    let text = text.to_lowercase();
    let mut total = 0;
    for word in query.split_whitespace() {
        total += word_score(&word.to_lowercase(), &text)?;
    }
    Some(total)
}

fn word_score(word: &str, text: &str) -> Option<i64> {
    if let Some(at) = text.find(word) {
        let starts_word = at == 0
            || text[..at]
                .chars()
                .next_back()
                .is_some_and(|c| !c.is_alphanumeric());
        let early = 20 - i64::try_from(at.min(20)).unwrap_or(20);
        return Some(100 + if starts_word { 50 } else { 0 } + early);
    }
    // Letters in order; each gap between them costs a little.
    let mut chars = text.chars();
    let mut gaps = 0;
    for wanted in word.chars() {
        let mut skipped = 0;
        loop {
            match chars.next() {
                Some(c) if c == wanted => break,
                Some(_) => skipped += 1,
                None => return None,
            }
        }
        if skipped > 0 {
            gaps += 1;
        }
    }
    Some(50 - gaps.min(40))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_word_must_match_in_any_order() {
        assert!(score("user get", "GET Get user · Users API").is_some());
        assert!(score("user delete", "GET Get user").is_none());
        assert_eq!(score("", "anything"), Some(0));
    }

    #[test]
    fn substrings_beat_scattered_letters_and_word_starts_win() {
        let exact = score("user", "Get user").expect("match");
        let scattered = score("gtusr", "Get user").expect("match");
        let inside = score("ser", "Get user").expect("match");
        assert!(exact > inside, "{exact} > {inside}");
        assert!(inside > scattered, "{inside} > {scattered}");
        assert!(score("xyz", "Get user").is_none());
    }

    #[test]
    fn case_and_non_ascii() {
        assert!(score("ÆRØ", "Ærøskøbing orders").is_some());
        assert!(score("POST", "post /orders").is_some());
    }
}
