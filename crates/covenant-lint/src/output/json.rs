//! JSON output for lint findings (one JSON object per line, NDJSON).

use crate::framework::Finding;
use crate::output::human::byte_to_line_col;

pub fn emit_findings(findings: &[Finding], source_path: &str, source_text: &str) {
    for f in findings {
        println!("{}", render_finding(f, source_path, source_text));
    }
}

pub fn render_finding(f: &Finding, source_path: &str, source_text: &str) -> String {
    let (line_start, col_start) = byte_to_line_col(source_text, f.span.start as usize);
    let (line_end, col_end) = byte_to_line_col(source_text, f.span.end as usize);

    let mut spans = vec![serde_json::json!({
        "file": source_path,
        "line_start": line_start,
        "column_start": col_start,
        "line_end": line_end,
        "column_end": col_end,
        "label": f.message,
        "is_primary": true,
    })];

    for (span, label) in &f.secondary_spans {
        let (ls, cs) = byte_to_line_col(source_text, span.start as usize);
        let (le, ce) = byte_to_line_col(source_text, span.end as usize);
        spans.push(serde_json::json!({
            "file": source_path,
            "line_start": ls,
            "column_start": cs,
            "line_end": le,
            "column_end": ce,
            "label": label,
            "is_primary": false,
        }));
    }

    serde_json::json!({
        "code": f.detector_code,
        "severity": f.severity.as_str(),
        "message": f.message,
        "spans": spans,
        "help": f.help,
    })
    .to_string()
}
