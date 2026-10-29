//! Fuzzy matching used to rank palette entries.

/// Case-insensitive subsequence match; higher is better, `None` if no match.
/// Consecutive and word-start matches score higher, so "sr" ranks
/// "Split right" above "Select mode … resize".
pub fn fuzzy_score(query: &str, candidate: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let cand: Vec<char> = candidate.to_lowercase().chars().collect();
    let mut score = 0;
    let mut pos = 0;
    let mut prev: Option<usize> = None;
    for q in query.to_lowercase().chars().filter(|c| !c.is_whitespace()) {
        let found = (pos..cand.len()).find(|&i| cand[i] == q)?;
        score += 1;
        if prev == Some(found.wrapping_sub(1)) {
            score += 5;
        }
        if found == 0 || matches!(cand[found - 1], ' ' | '-' | ':' | '/') {
            score += 3;
        }
        score -= (found - pos).min(5) as i32;
        prev = Some(found);
        pos = found + 1;
    }
    Some(score)
}
