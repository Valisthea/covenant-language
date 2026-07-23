//! Inventory test for Tour lesson starter codes + solutions.
//!
//! Sprint 27 Phase 27.1.c: parallel to inventory_examples.rs but for
//! the Tour. Extracts each `codeStarter` and `codeSolution` template
//! literal from the M1/M2/M3 lesson .ts files via a small regex pass,
//! then attempts compilation. Reports per-lesson pass/fail.
//!
//! Run with:
//!
//!   cargo test -p covenant-wasm-bindings --test inventory_tour \
//!     -- --nocapture --ignored

use covenant_wasm_bindings::adapt::compile_evm;
use covenant_wasm_bindings::result::JsLevel;

use std::fs;
use std::path::PathBuf;

fn lessons_dir() -> Option<PathBuf> {
    let candidates = [
        "../../../covenant-playground/src/tour/lessons",
        "../../covenant-playground/src/tour/lessons",
        "../covenant-playground/src/tour/lessons",
    ];
    for c in &candidates {
        let p = PathBuf::from(c);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

/// Parse a TypeScript file containing lesson definitions; extract every
/// (lesson_id, code_kind, source) triple where code_kind is "starter"
/// or "solution".
///
/// Naive approach: scan for `codeStarter:` or `codeSolution:` followed
/// by a backtick-delimited template literal. Sufficient for the
/// existing M1/M2/M3 file format. Future format changes may need a
/// real TS parser.
fn extract_lesson_codes(ts_source: &str) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let mut current_id = String::new();

    // Track lesson ID: look for `id: 'M1L3'` or `id: "M1L3"` patterns.
    // When we hit a code field, attach to the current id.
    let id_re = regex_lite("id:\\s*['\"]([^'\"]+)['\"]");
    let starter_marker = "codeStarter:";
    let solution_marker = "codeSolution:";

    let mut chars = ts_source.char_indices().peekable();
    let bytes = ts_source.as_bytes();

    let mut i = 0;
    while i < bytes.len() {
        // Try to match an id near here.
        if let Some(rest) = ts_source.get(i..) {
            if let Some(captured) = id_re.captures(rest) {
                if captured.start <= 80 {
                    // Reasonably close to the cursor — accept.
                    current_id = captured.text.clone();
                }
            }
            // Look for a starter or solution marker at this byte.
            for (marker, kind) in [(starter_marker, "starter"), (solution_marker, "solution")] {
                if let Some(after_marker) = rest.strip_prefix(marker) {
                    // Skip the marker; find the opening backtick.
                    if let Some(tick_off) = after_marker.find('`') {
                        let body_start = i + marker.len() + tick_off + 1;
                        // Find the closing backtick — may be many chars later.
                        if let Some(close_rel) = ts_source[body_start..].find('`') {
                            let body_end = body_start + close_rel;
                            let body = ts_source[body_start..body_end].to_string();
                            out.push((current_id.clone(), kind.to_string(), body));
                            i = body_end + 1;
                            continue;
                        }
                    }
                }
            }
        }
        let _ = chars.next();
        i += 1;
        if i > bytes.len() {
            break;
        }
    }

    out
}

#[test]
#[ignore = "reads files from sibling covenant-playground repo; run explicitly"]
fn inventory_tour_lessons() {
    let dir = match lessons_dir() {
        Some(d) => d,
        None => {
            eprintln!("tour lessons dir not found — run from covenant repo root");
            return;
        }
    };

    let mut files: Vec<_> = fs::read_dir(&dir)
        .expect("read lessons dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            let n = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            n.starts_with("M") && n.ends_with(".ts")
        })
        .collect();
    files.sort();

    let mut pass = Vec::<String>::new();
    let mut fail = Vec::<(String, String)>::new();
    let mut total = 0usize;

    for f in &files {
        let ts = fs::read_to_string(f).expect("read lesson file");
        let codes = extract_lesson_codes(&ts);
        for (id, kind, source) in codes {
            total += 1;
            // Skip empty extractions (parser miss).
            if source.trim().is_empty() {
                fail.push((format!("{id} ({kind})"), "empty extraction".into()));
                continue;
            }
            // Some Tour starter codes deliberately have a TODO that the user
            // is supposed to fill in — those will fail to compile by design.
            // But a "solution" should always compile. We report both, the
            // operator separates "expected fail" from "actually broken".
            let r = compile_evm(&source);
            let errs: Vec<_> = r
                .diagnostics
                .iter()
                .filter(|d| d.level == JsLevel::Error)
                .collect();
            if r.ok && errs.is_empty() {
                pass.push(format!("{id} ({kind})"));
            } else {
                let msg = errs
                    .first()
                    .map(|d| format!("L{}:{} {} — {}", d.line, d.column, d.code, d.message))
                    .unwrap_or_else(|| "(no error msg)".into());
                fail.push((format!("{id} ({kind})"), msg));
            }
        }
    }

    println!(
        "\n=== TOUR LESSON CODES INVENTORY ({} total: starter+solution per lesson) ===",
        total
    );
    println!("\n✅ COMPILES CLEAN ({}):", pass.len());
    for n in &pass {
        println!("   ✓ {n}");
    }
    println!("\n❌ FAILS TO COMPILE ({}):", fail.len());
    for (n, e) in &fail {
        println!("   ✗ {n}");
        println!("      {e}");
    }
    println!("\n=== TOTAL: {}/{} compile ===\n", pass.len(), total);
}

// Tiny regex-lite: just captures the first match of `id: 'X'` near the
// cursor. Avoids pulling regex dep into the test crate.
struct RegexLiteMatch {
    text: String,
    start: usize,
}

struct RegexLite {
    pattern_kind: PatternKind,
}

enum PatternKind {
    IdSingleOrDouble,
}

fn regex_lite(_pat: &str) -> RegexLite {
    // Hard-coded to match `id: 'X'` or `id: "X"` since that's all we need.
    RegexLite {
        pattern_kind: PatternKind::IdSingleOrDouble,
    }
}

impl RegexLite {
    fn captures(&self, hay: &str) -> Option<RegexLiteMatch> {
        match self.pattern_kind {
            PatternKind::IdSingleOrDouble => {
                // Find first `id:` followed by quote + content + matching quote.
                let needle = "id:";
                let pos = hay.find(needle)?;
                let after = &hay[pos + needle.len()..];
                let after_trim = after.trim_start();
                let leading_ws = after.len() - after_trim.len();
                let q = after_trim.chars().next()?;
                if q != '\'' && q != '"' {
                    return None;
                }
                let body = &after_trim[1..];
                let close = body.find(q)?;
                let text = body[..close].to_string();
                Some(RegexLiteMatch {
                    text,
                    start: pos + needle.len() + leading_ws + 1,
                })
            }
        }
    }
}
