//! Edit-distance suggestion helper for unresolved-name diagnostics.

/// Classic Levenshtein distance. Quadratic in length, but identifiers are tiny
/// and we only run this on the error path.
pub fn levenshtein(a: &str, b: &str) -> usize {
    let ac: Vec<char> = a.chars().collect();
    let bc: Vec<char> = b.chars().collect();
    let n = ac.len();
    let m = bc.len();
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut curr = vec![0usize; m + 1];
    for i in 1..=n {
        curr[0] = i;
        for j in 1..=m {
            let cost = if ac[i - 1] == bc[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[m]
}

/// Return the best single suggestion for `target` from `candidates`, or `None`
/// if no candidate is close enough.
///
/// Rules (match Phase 3 spec):
/// * edit distance ≤ 2, and
/// * the candidate's own length is ≥ 3 (to avoid noise like `x` → `y`).
pub fn suggest<'a, I>(target: &str, candidates: I) -> Option<&'a str>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut best: Option<(&str, usize)> = None;
    for cand in candidates {
        if cand.len() < 3 {
            continue;
        }
        let d = levenshtein(target, cand);
        if d <= 2 && best.is_none_or(|(_, bd)| d < bd) {
            best = Some((cand, d));
        }
    }
    best.map(|(s, _)| s)
}
