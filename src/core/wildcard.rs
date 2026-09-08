use std::collections::BTreeSet;
use regex::Regex;

/// Whether a package argument uses a wildcard (`*` glob or `?` single char).
pub fn has_wildcard(s: &str) -> bool {
    s.contains('*') || s.contains('?')
}

/// Compile a shell-style pattern (`pkg*`, `*core*`, `lib?.so`) into a regex
/// so `*` matches any run of characters and `?` matches a single character.
pub fn pattern_to_regex(pattern: &str) -> Regex {
    let mut out = String::with_capacity(pattern.len() + 8);
    out.push('^');
    for ch in pattern.chars() {
        match ch {
            '*' => out.push_str(".*"),
            '?' => out.push('.'),
            // Escape every regex metacharacter so it is matched literally.
            c if r".+()|[]{}^$\\".contains(c) => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out.push('$');
    Regex::new(&out).expect("wildcard pattern compiles to valid regex")
}

/// Match a wildcard pattern against package names, returning the matches
/// sorted and deduplicated. Non-wildcard patterns yield nothing here — the
/// caller passes exact names through untouched.
pub fn expand<'a>(pattern: &str, candidates: impl Iterator<Item = &'a str>) -> Vec<String> {
    let re = pattern_to_regex(pattern);
    let mut matched = BTreeSet::new();
    for name in candidates {
        if re.is_match(name) {
            matched.insert(name.to_string());
        }
    }
    matched.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expand_pattern(pattern: &str, names: &[&str]) -> Vec<String> {
        expand(pattern, names.iter().copied())
    }

    #[test]
    fn test_has_wildcard() {
        assert!(has_wildcard("pkg*"));
        assert!(has_wildcard("*pkg"));
        assert!(has_wildcard("p?g"));
        assert!(!has_wildcard("pkg-1"));
    }

    #[test]
    fn test_suffix_pattern_matches_prefix_group() {
        let names = ["pkg-1", "pkg-2", "pkg-3", "otherpkg"];
        assert_eq!(expand_pattern("pkg*", &names), vec!["pkg-1", "pkg-2", "pkg-3"]);
    }

    #[test]
    fn test_prefix_glob() {
        let names = ["alpha", "beta", "alphabet"];
        assert_eq!(expand_pattern("*alpha", &names), vec!["alpha"]);
        assert_eq!(expand_pattern("alpha*", &names), vec!["alpha", "alphabet"]);
    }

    #[test]
    fn test_infix_glob() {
        let names = ["linux-header", "linux-image", "other"];
        assert_eq!(expand_pattern("*image*", &names), vec!["linux-image"]);
    }

    #[test]
    fn test_single_char_wildcard() {
        let names = ["pkg-a", "pkg-b", "pkg-ab"];
        assert_eq!(expand_pattern("pkg-?", &names), vec!["pkg-a", "pkg-b"]);
    }

    #[test]
    fn test_no_matches_is_empty() {
        let names = ["pkg-1", "pkg-2"];
        assert!(expand_pattern("zzz*", &names).is_empty());
    }

    #[test]
    fn test_regex_metachars_are_literal() {
        let names = ["a+b", "axb"];
        assert_eq!(expand_pattern("a+b", &names), vec!["a+b"]);
    }

    #[test]
    fn test_matches_are_deduped_and_sorted() {
        let names = ["pkg-2", "pkg-1", "pkg-2"];
        assert_eq!(expand_pattern("pkg-*", &names), vec!["pkg-1", "pkg-2"]);
    }
}