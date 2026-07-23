//! Ariadne-based diagnostic rendering per Doc 9 §12.

use ariadne::{Color, Config, Label, Report, ReportKind, Source};
use covenant_diag::{Diagnostic, DiagnosticLevel};

use crate::output::OutputFormat;

/// Render a list of diagnostics to stderr.
///
/// In human mode, ariadne is used for source-context rendering.
/// In JSON mode, one JSON object per diagnostic is printed to stdout.
pub fn emit_diagnostics(
    diagnostics: &[Diagnostic],
    source_path: &str,
    source_text: &str,
    format: OutputFormat,
    use_color: bool,
) {
    match format {
        OutputFormat::Human => {
            for d in diagnostics {
                render_human(d, source_path, source_text, use_color);
            }
        }
        OutputFormat::Json => {
            for d in diagnostics {
                println!("{}", render_json(d, source_path, source_text));
            }
        }
    }
}

fn render_human(d: &Diagnostic, source_path: &str, source_text: &str, use_color: bool) {
    let kind = match d.level {
        DiagnosticLevel::Error => ReportKind::Error,
        DiagnosticLevel::Warning => ReportKind::Warning,
        DiagnosticLevel::Note => ReportKind::Advice,
    };

    let code_str = match d.level {
        DiagnosticLevel::Warning => format!("W{:03}", d.code.0),
        _ => format!("{}", d.code),
    };

    let span_start = d.span.start as usize;
    let span_end = d.span.end as usize;

    let config = Config::default().with_color(use_color);

    let mut builder = Report::build(kind, source_path, span_start)
        .with_config(config)
        .with_code(&code_str)
        .with_message(&d.message);

    if span_end > span_start {
        let label_color = match d.level {
            DiagnosticLevel::Error => Color::Red,
            DiagnosticLevel::Warning => Color::Yellow,
            DiagnosticLevel::Note => Color::Cyan,
        };
        builder = builder.with_label(
            Label::new((source_path, span_start..span_end))
                .with_message(&d.message)
                .with_color(label_color),
        );
    }

    if let Some(help) = &d.help {
        builder = builder.with_help(help);
    }

    let report = builder.finish();
    let cache = (source_path, Source::from(source_text));

    let mut buf = Vec::new();
    if report.write(cache, &mut buf).is_ok() {
        eprint!("{}", String::from_utf8_lossy(&buf));
    } else {
        // Fallback: plain text
        let level = match d.level {
            DiagnosticLevel::Error => "error",
            DiagnosticLevel::Warning => "warning",
            DiagnosticLevel::Note => "note",
        };
        eprintln!("{level}[{code_str}]: {}", d.message);
        if let Some(help) = &d.help {
            eprintln!("  help: {help}");
        }
    }
}

fn render_json(d: &Diagnostic, source_path: &str, source_text: &str) -> String {
    let level_str = match d.level {
        DiagnosticLevel::Error => "error",
        DiagnosticLevel::Warning => "warning",
        DiagnosticLevel::Note => "note",
    };
    let code_str = match d.level {
        DiagnosticLevel::Warning => format!("W{:03}", d.code.0),
        _ => format!("{}", d.code),
    };

    let (line_start, col_start) = byte_to_line_col(source_text, d.span.start as usize);
    let (line_end, col_end) = byte_to_line_col(source_text, d.span.end as usize);

    let obj = serde_json::json!({
        "level": level_str,
        "code": code_str,
        "message": d.message,
        "spans": [{
            "file": source_path,
            "line_start": line_start,
            "column_start": col_start,
            "line_end": line_end,
            "column_end": col_end,
            "label": d.message,
        }],
        "help": d.help,
    });
    obj.to_string()
}

/// Convert a byte offset to (line, column), both 1-indexed.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_to_line_col_first_char() {
        assert_eq!(byte_to_line_col("hello\nworld", 0), (1, 1));
    }

    #[test]
    fn byte_to_line_col_second_line() {
        // offset 6 = 'w' on line 2
        assert_eq!(byte_to_line_col("hello\nworld", 6), (2, 1));
    }

    #[test]
    fn byte_to_line_col_clamped() {
        let src = "hi";
        assert_eq!(byte_to_line_col(src, 100), (1, 3));
    }
}
