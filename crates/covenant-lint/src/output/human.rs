//! Ariadne-based human-readable output for lint findings.

use ariadne::{Color, Config, Label, Report, ReportKind, Source};

use crate::framework::{Finding, Severity};

pub fn emit_findings(findings: &[Finding], source_path: &str, source_text: &str, use_color: bool) {
    for finding in findings {
        render_finding(finding, source_path, source_text, use_color);
    }
}

fn render_finding(f: &Finding, source_path: &str, source_text: &str, use_color: bool) {
    let kind = match f.severity {
        Severity::Critical => ReportKind::Error,
        Severity::Warning => ReportKind::Warning,
        Severity::Info => ReportKind::Advice,
    };
    let primary_color = match f.severity {
        Severity::Critical => Color::Red,
        Severity::Warning => Color::Yellow,
        Severity::Info => Color::Cyan,
    };

    let span_start = f.span.start as usize;
    let span_end = f.span.end as usize;
    let config = Config::default().with_color(use_color);

    let mut builder = Report::build(kind, source_path, span_start)
        .with_config(config)
        .with_code(f.detector_code)
        .with_message(&f.message);

    if span_end > span_start {
        builder = builder.with_label(
            Label::new((source_path, span_start..span_end))
                .with_message(&f.message)
                .with_color(primary_color),
        );
    }

    for (span, label) in &f.secondary_spans {
        let ss = span.start as usize;
        let se = span.end as usize;
        if se > ss {
            builder = builder.with_label(
                Label::new((source_path, ss..se))
                    .with_message(*label)
                    .with_color(Color::Blue),
            );
        }
    }

    if let Some(help) = f.help {
        builder = builder.with_help(help);
    }

    let report = builder.finish();
    let cache = (source_path, Source::from(source_text));

    let mut buf = Vec::new();
    if report.write(cache, &mut buf).is_ok() {
        eprint!("{}", String::from_utf8_lossy(&buf));
    } else {
        let level = f.severity.as_str();
        eprintln!("{level}[{}]: {}", f.detector_code, f.message);
        if let Some(help) = f.help {
            eprintln!("  help: {help}");
        }
    }
}

/// (line, column) both 1-indexed from a byte offset.
pub fn byte_to_line_col(source: &str, offset: usize) -> (usize, usize) {
    let clamped = offset.min(source.len());
    let mut line = 1usize;
    let mut col = 1usize;
    for (i, c) in source.char_indices() {
        if i >= clamped {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}
