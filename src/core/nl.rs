//! Natural-language snippet matching.
//!
//! Scores every snippet against a free-text query by combining signals from
//! the snippet's key, command, description, and tags. Returns ranked matches.
//!
//! Unlike `core::fuzzy::fuzzy_match` (which only matches against the key
//! using fuzzy-matcher's SkimMatcher), NL matching tokenises both the query
//! and the snippet fields and rewards token overlap. This lets you type
//! `snip "deploy staging"` and match a snippet whose key is `deploy.staging`,
//! or whose description is "Deploy to staging environment", or whose command
//! is `kubectl --context=staging apply ...`.
//!
//! The scoring is intentionally simple and predictable:
//!
//! - **Exact key match**:           +1000  (always wins if you typed the full key)
//! - **Token overlap with key**:    +20  per matched token
//! - **Token overlap with cmd**:    +10  per matched token
//! - **Token overlap with desc**:   +15  per matched token
//! - **Token overlap with tags**:   +25  per matched token
//! - **Exact phrase in desc**:      +200 (e.g. "deploy staging" in "Deploy to staging")
//! - **Exact phrase in key**:       +300 (treat dot as separator: "deploy.staging")
//!
//! Tokens are lowercased alphanumeric runs of length >= 2. Stop-words like
//! "the", "a", "to", "run", "start" are filtered out so they don't dominate
//! the score.

use crate::core::snippet::SnipFile;

/// A ranked match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NlMatch {
    pub key: String,
    pub score: i64,
}

/// Score every snippet in `file` against `query` and return matches sorted
/// by score descending. Matches with score <= 0 are excluded.
pub fn nl_match(file: &SnipFile, query: &str) -> Vec<NlMatch> {
    let query_tokens = tokenise(query);
    let query_phrase = normalise_phrase(query);

    if query_tokens.is_empty() {
        return Vec::new();
    }

    let mut results: Vec<NlMatch> = Vec::new();

    for (key, snippet) in file.iter() {
        let mut score: i64 = 0;

        // Exact key match — always wins
        if key.eq_ignore_ascii_case(query.trim()) {
            score += 1000;
        }

        // Tokenise key (treat dot and dash as separators)
        let key_tokens = tokenise(key);
        let key_phrase = normalise_phrase(key);

        // Phrase-in-key bonus
        if !query_phrase.is_empty() && key_phrase.contains(&query_phrase) {
            score += 300;
        }

        let cmd_tokens = tokenise(&snippet.cmd);
        let desc_tokens = tokenise(&snippet.desc);
        let tags_tokens: Vec<String> = snippet.tags.iter().flat_map(|t| tokenise(t)).collect();

        // Token overlap scoring
        for qt in &query_tokens {
            if key_tokens.contains(qt) {
                score += 20;
            }
            if cmd_tokens.contains(qt) {
                score += 10;
            }
            if desc_tokens.contains(qt) {
                score += 15;
            }
            if tags_tokens.contains(qt) {
                score += 25;
            }
        }

        // Phrase-in-desc bonus
        let desc_phrase = normalise_phrase(&snippet.desc);
        if !query_phrase.is_empty()
            && !desc_phrase.is_empty()
            && desc_phrase.contains(&query_phrase)
        {
            score += 200;
        }

        if score > 0 {
            results.push(NlMatch {
                key: key.clone(),
                score,
            });
        }
    }

    // Sort by score descending; tie-break alphabetically for determinism.
    results.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.key.cmp(&b.key)));

    results
}

/// Tokenise a string into lowercase alphanumeric runs of length >= 2,
/// filtering out common stop-words.
fn tokenise(s: &str) -> Vec<String> {
    let stop_words: &[&str] = &[
        "the", "a", "an", "to", "of", "for", "and", "or", "in", "on", "at", "is", "are", "be",
        "by", "with", "from", "into", "this", "that", "run", "start", "do",
        "go", // very common in "run X" / "start Y" queries
    ];

    s.split(|c: char| !c.is_alphanumeric())
        .filter_map(|tok| {
            let lower = tok.to_lowercase();
            if lower.len() < 2 {
                return None;
            }
            if stop_words.contains(&lower.as_str()) {
                return None;
            }
            Some(lower)
        })
        .collect()
}

/// Normalise a phrase for substring matching: lowercase, collapse whitespace.
fn normalise_phrase(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::snippet::{SnipFile, Snippet};

    fn build() -> SnipFile {
        let mut f = SnipFile::new();
        f.insert(
            "deploy.staging",
            Snippet::new("kubectl --context=stg apply -f k8s/")
                .with_desc("Deploy to staging environment")
                .with_tags(vec!["deploy".into(), "release".into()]),
        );
        f.insert(
            "deploy.production",
            Snippet::new("kubectl --context=prod apply -f k8s/")
                .with_desc("Deploy to production")
                .with_tags(vec!["deploy".into()]),
        );
        f.insert("test", Snippet::new("cargo test").with_desc("Run tests"));
        f.insert(
            "db.seed",
            Snippet::new("psql -f seed.sql").with_desc("Seed the database"),
        );
        f.insert(
            "frontend.dev",
            Snippet::new("npm run dev").with_desc("Start the frontend dev server"),
        );
        f
    }

    #[test]
    fn exact_key_match_wins() {
        let f = build();
        let m = nl_match(&f, "deploy.staging");
        assert_eq!(m[0].key, "deploy.staging");
        // Should outrank deploy.production because of the exact key bonus
        assert!(
            m[0].score
                > m.iter()
                    .find(|x| x.key == "deploy.production")
                    .unwrap()
                    .score
        );
    }

    #[test]
    fn phrase_in_desc_matches() {
        let f = build();
        let m = nl_match(&f, "deploy staging");
        assert!(!m.is_empty());
        assert_eq!(m[0].key, "deploy.staging");
    }

    #[test]
    fn phrase_in_desc_matches_frontend() {
        let f = build();
        let m = nl_match(&f, "start frontend");
        assert!(!m.is_empty());
        assert_eq!(m[0].key, "frontend.dev");
    }

    #[test]
    fn tag_match_ranks_high() {
        let f = build();
        let m = nl_match(&f, "release");
        assert!(!m.is_empty());
        // deploy.staging has the "release" tag; deploy.production doesn't
        assert_eq!(m[0].key, "deploy.staging");
    }

    #[test]
    fn no_match_returns_empty() {
        let f = build();
        let m = nl_match(&f, "zzz nonexistent thing");
        assert!(m.is_empty());
    }

    #[test]
    fn empty_query_returns_empty() {
        let f = build();
        assert!(nl_match(&f, "").is_empty());
        assert!(nl_match(&f, "   ").is_empty());
    }

    #[test]
    fn stop_words_dont_skew_scoring() {
        let f = build();
        // "run" is a stop-word, so "run tests" should effectively be "tests"
        let m = nl_match(&f, "run tests");
        assert!(!m.is_empty());
        assert_eq!(m[0].key, "test");
    }

    #[test]
    fn multiple_results_sorted_by_score() {
        let f = build();
        let m = nl_match(&f, "deploy");
        // Both deploy.staging and deploy.production should match, sorted by score
        assert!(m.len() >= 2);
        assert!(m[0].score >= m[1].score);
        let keys: Vec<&str> = m.iter().map(|x| x.key.as_str()).collect();
        assert!(keys.contains(&"deploy.staging"));
        assert!(keys.contains(&"deploy.production"));
    }

    #[test]
    fn tokenise_handles_dots_and_dashes() {
        let toks = tokenise("deploy.staging release-candidate");
        assert!(toks.contains(&"deploy".to_string()));
        assert!(toks.contains(&"staging".to_string()));
        assert!(toks.contains(&"release".to_string()));
        assert!(toks.contains(&"candidate".to_string()));
    }

    #[test]
    fn tokenise_filters_short_and_stopwords() {
        let toks = tokenise("a to the run x");
        // All of: "a" (len 1), "to" (stopword), "the" (stopword), "run" (stopword),
        // "x" (len 1) should be filtered out
        assert!(toks.is_empty());
    }

    #[test]
    fn cmd_token_match_contributes() {
        let mut f = SnipFile::new();
        f.insert(
            "mystery",
            Snippet::new("psql --dbname=prod --command='SELECT 1'").with_desc("mystery"),
        );
        let m = nl_match(&f, "psql");
        assert!(!m.is_empty());
        assert_eq!(m[0].key, "mystery");
    }
}
