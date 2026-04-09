/// Simple wildcard matching.
/// Supports `*` (match zero or more characters) and `?` (match exactly one character).
/// The wildcard characters match any character(s) except path separator /.
pub fn wildcard_match(pattern: &str, text: &str) -> bool {
    let pat: Vec<char> = pattern.chars().collect();
    let txt: Vec<char> = text.chars().collect();
    let tlen = txt.len();

    // dp[j] means pattern[0..i] matches text[0..j]
    let mut dp = vec![false; tlen + 1];
    dp[0] = true;

    for &pc in &pat {
        if pc == '*' {
            // '*' can extend any previous match forward
            for j in 1..=tlen {
                dp[j] = dp[j] || dp[j - 1];
            }
        } else {
            // Process right-to-left so we don't use values updated in this row
            for j in (1..=tlen).rev() {
                dp[j] = dp[j - 1] && (pc == '?' || pc == txt[j - 1]);
            }
            dp[0] = false;
        }
    }

    dp[tlen]
}

pub fn wildcard_filter(candidates: Vec<String>, patterns: Vec<String>) -> Vec<String> {
    candidates
        .into_iter()
        .filter(|c| patterns.iter().any(|p| wildcard_match(p, c)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match() {
        assert!(wildcard_match("hello", "hello"));
        assert!(!wildcard_match("hello", "world"));
    }

    #[test]
    fn star_matches_everything() {
        assert!(wildcard_match("*", ""));
        assert!(wildcard_match("*", "anything"));
        assert!(wildcard_match("*", "a/b/c"));
    }

    #[test]
    fn star_prefix_suffix() {
        assert!(wildcard_match("*.txt", "file.txt"));
        assert!(!wildcard_match("*.txt", "file.rs"));
        assert!(wildcard_match("hello*", "hello"));
        assert!(wildcard_match("hello*", "helloworld"));
    }

    #[test]
    fn star_in_middle() {
        assert!(wildcard_match("he*lo", "hello"));
        assert!(wildcard_match("he*lo", "helo"));
        assert!(wildcard_match("he*lo", "he123lo"));
        assert!(!wildcard_match("he*lo", "he123lx"));
    }

    #[test]
    fn multiple_stars() {
        assert!(wildcard_match("*a*b*", "xaybz___b"));
        assert!(wildcard_match("*a*b*", "ab"));
        assert!(!wildcard_match("*a*b*", "acdc"));
    }

    #[test]
    fn question_mark() {
        assert!(wildcard_match("h?llo", "hello"));
        assert!(wildcard_match("h?llo", "hallo"));
        assert!(!wildcard_match("h?llo", "hllo"));
        assert!(!wildcard_match("h?llo", "heello"));
    }

    #[test]
    fn mixed_wildcards() {
        assert!(wildcard_match("h?l*", "hello"));
        assert!(wildcard_match("h?l*", "hal"));
        assert!(!wildcard_match("h?l*", "ha"));
        assert!(wildcard_match("*?", "a"));
        assert!(!wildcard_match("*?", ""));
    }

    #[test]
    fn empty_pattern_and_text() {
        assert!(wildcard_match("", ""));
        assert!(!wildcard_match("", "a"));
        assert!(!wildcard_match("a", ""));
    }

    #[test]
    fn subvolume_style_patterns() {
        // Typical usage: matching btrfs subvolume names
        assert!(wildcard_match("@home*", "@home"));
        assert!(wildcard_match("@home*", "@home_snapshot"));
        assert!(wildcard_match("@*", "@home"));
        assert!(wildcard_match("@*", "@root"));
        assert!(!wildcard_match("@home", "@root"));
    }
}
