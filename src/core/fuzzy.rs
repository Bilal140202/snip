use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

/// A single fuzzy-match result with a score.
#[derive(Debug, Clone)]
pub struct FuzzyResult {
    pub key: String,
    pub score: i64,
}

/// Fuzzy-match `query` against a list of `keys`.
///
/// Returns results sorted by score descending (best match first).
/// An empty query returns all keys (score 0).
/// Results with a score ≤ 0 are excluded (unless query is empty).
pub fn fuzzy_match(query: &str, keys: &[String]) -> Vec<FuzzyResult> {
    let matcher = SkimMatcherV2::default();

    if query.is_empty() {
        return keys
            .iter()
            .map(|key| FuzzyResult {
                key: key.clone(),
                score: 0,
            })
            .collect();
    }

    let mut results: Vec<FuzzyResult> = keys
        .iter()
        .filter_map(|key| {
            let score = matcher.fuzzy_match(key, query)?;
            if score > 0 {
                Some(FuzzyResult {
                    key: key.clone(),
                    score,
                })
            } else {
                None
            }
        })
        .collect();

    results.sort_by(|a, b| b.score.cmp(&a.score));
    results
}

/// Find the single best fuzzy match. Returns `None` if nothing matches.
pub fn fuzzy_best(query: &str, keys: &[String]) -> Option<String> {
    let results = fuzzy_match(query, keys);
    results.into_iter().next().map(|r| r.key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match() {
        let keys = vec!["build".into(), "test".into(), "deploy".into()];
        let results = fuzzy_match("build", &keys);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, "build");
    }

    #[test]
    fn fuzzy_match_works() {
        let keys = vec!["build-release".into(), "build-debug".into(), "test".into()];
        let results = fuzzy_match("bldrel", &keys);
        assert!(!results.is_empty());
        assert_eq!(results[0].key, "build-release");
    }

    #[test]
    fn no_match_returns_empty() {
        let keys = vec!["build".into(), "test".into()];
        let results = fuzzy_match("xyz", &keys);
        assert!(results.is_empty());
    }
}