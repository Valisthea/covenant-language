//! `covenant fmt`: canonical source formatter.
//!
//! Parses source → pretty-prints via covenant_parser::printer.
//! Comments are discarded (the lexer does not preserve them).

use std::io::Read as IoRead;
use std::path::{Path, PathBuf};

use clap::Args;
use covenant_diag::SourceId;

use crate::error::CliError;

#[derive(Args, Debug, Clone)]
pub struct FmtArgs {
    /// Source file or directory to format (omit for project mode).
    #[arg(value_name = "TARGET")]
    pub target: Option<PathBuf>,

    /// Check formatting without writing; exit 1 if any file needs reformatting.
    #[arg(long)]
    pub check: bool,

    /// Print unified diff to stdout instead of rewriting files.
    #[arg(long)]
    pub diff: bool,

    /// Read from stdin, write to stdout.
    #[arg(long)]
    pub stdin: bool,
}

pub fn run(args: FmtArgs, _verbose: u8) -> Result<(), CliError> {
    if args.stdin {
        return run_stdin(&args);
    }

    let files = collect_targets(&args)?;
    if files.is_empty() {
        if args.check {
            return Ok(());
        }
        eprintln!("warning: no .cov files found to format");
        return Ok(());
    }

    let mut needs_reformat = false;
    for path in &files {
        let original = std::fs::read_to_string(path).map_err(|e| {
            CliError::Io(std::io::Error::new(
                e.kind(),
                format!("cannot read `{}`: {e}", path.display()),
            ))
        })?;

        let formatted = format_source(&original, path)?;

        if original == formatted {
            continue; // already canonical
        }

        needs_reformat = true;
        if args.check {
            eprintln!("would reformat: {}", path.display());
        } else if args.diff {
            print!(
                "{}",
                make_diff(&original, &formatted, &path.to_string_lossy())
            );
        } else {
            std::fs::write(path, &formatted).map_err(|e| {
                CliError::Io(std::io::Error::new(
                    e.kind(),
                    format!("cannot write `{}`: {e}", path.display()),
                ))
            })?;
            println!("formatted: {}", path.display());
        }
    }

    if args.check && needs_reformat {
        return Err(CliError::FmtCheckFailed);
    }
    Ok(())
}

fn run_stdin(args: &FmtArgs) -> Result<(), CliError> {
    let mut src = String::new();
    std::io::stdin()
        .read_to_string(&mut src)
        .map_err(CliError::Io)?;
    let formatted = format_source(&src, Path::new("<stdin>"))?;
    if args.check {
        if src != formatted {
            return Err(CliError::FmtCheckFailed);
        }
        return Ok(());
    }
    if args.diff {
        print!("{}", make_diff(&src, &formatted, "<stdin>"));
        return Ok(());
    }
    print!("{formatted}");
    Ok(())
}

/// Format Covenant source text using the AST pretty-printer.
pub fn format_source(source: &str, _path: &Path) -> Result<String, CliError> {
    let sid = SourceId::new(0);
    let (tokens, _lex_diags) = covenant_lexer::tokenize(source, sid);
    let (file_opt, _parse_diags) = covenant_parser::parse(&tokens, sid);
    match file_opt {
        Some(file) => Ok(covenant_parser::printer::pretty_print(&file)),
        None => Err(CliError::CompileError),
    }
}

/// Collect .cov files to format.
fn collect_targets(args: &FmtArgs) -> Result<Vec<PathBuf>, CliError> {
    if let Some(t) = &args.target {
        if t.is_file() && t.extension().is_some_and(|e| e == "cov") {
            return Ok(vec![t.clone()]);
        }
        if t.is_dir() {
            return Ok(collect_cov_files(t));
        }
        // Path given but doesn't exist or isn't .cov
        if !t.exists() {
            return Err(CliError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("path not found: {}", t.display()),
            )));
        }
    }

    // Project mode: read manifest
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let manifest_path =
        covenant_manifest::Manifest::find_upward(&cwd).ok_or(CliError::ManifestNotFound)?;
    let manifest = covenant_manifest::Manifest::read(&manifest_path)?;
    let root = manifest_path
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();
    let src_root = manifest
        .project
        .source
        .root
        .clone()
        .unwrap_or_else(|| PathBuf::from("src"));
    Ok(collect_cov_files(&root.join(src_root)))
}

fn collect_cov_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                out.extend(collect_cov_files(&p));
            } else if p.extension().is_some_and(|e| e == "cov") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

// ---------------------------------------------------------------------------
// Minimal unified diff
// ---------------------------------------------------------------------------

fn make_diff(old: &str, new: &str, label: &str) -> String {
    if old == new {
        return String::new();
    }
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();

    let prefix = old_lines
        .iter()
        .zip(new_lines.iter())
        .take_while(|(a, b)| a == b)
        .count();
    let suffix_max = (old_lines.len() - prefix).min(new_lines.len() - prefix);
    let suffix = old_lines[prefix..]
        .iter()
        .rev()
        .zip(new_lines[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .take(suffix_max)
        .count();

    let old_end = old_lines.len() - suffix;
    let new_end = new_lines.len() - suffix;

    let ctx = 3usize;
    let ctx_start = prefix.saturating_sub(ctx);
    let ctx_end_old = (old_end + ctx).min(old_lines.len());

    let mut out = format!("--- {label}\n+++ {label}\n");
    out.push_str(&format!(
        "@@ -{},{} +{},{} @@\n",
        ctx_start + 1,
        ctx_end_old - ctx_start,
        ctx_start + 1,
        (new_end + ctx).min(new_lines.len()) - ctx_start,
    ));

    for line in &old_lines[ctx_start..prefix] {
        out.push_str(&format!(" {line}\n"));
    }
    for line in &old_lines[prefix..old_end] {
        out.push_str(&format!("-{line}\n"));
    }
    for line in &new_lines[prefix..new_end] {
        out.push_str(&format!("+{line}\n"));
    }
    for line in &old_lines[old_end..ctx_end_old] {
        out.push_str(&format!(" {line}\n"));
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn fmt(src: &str) -> String {
        format_source(src, Path::new("test.cov")).unwrap()
    }

    #[test]
    fn format_simple_record() {
        let src = "record R { x: amount\naction go() only caller { x = 1 }\nview get returns amount { x } }";
        let out = fmt(src);
        assert!(out.contains("record R {"));
        assert!(out.contains("x: amount"));
        assert!(out.contains("action go() only caller {"));
        // Printer normalizes `view get` to `view get()` (always emits parens).
        assert!(out.contains("view get()") && out.contains("returns amount"));
    }

    #[test]
    fn format_is_idempotent_hello() {
        let src = include_str!("../../../covenant-lexer/tests/fixtures/example_01_hello.cov");
        let once = fmt(src);
        let twice = fmt(&once);
        assert_eq!(once, twice, "fmt not idempotent on hello.cov");
    }

    #[test]
    fn format_is_idempotent_coin() {
        let src = include_str!("../../../covenant-lexer/tests/fixtures/example_02_coin.cov");
        let once = fmt(src);
        let twice = fmt(&once);
        assert_eq!(once, twice, "fmt not idempotent on coin.cov");
    }

    #[test]
    fn format_is_idempotent_ballot() {
        let src = include_str!("../../../covenant-lexer/tests/fixtures/example_03_open_ballot.cov");
        let once = fmt(src);
        let twice = fmt(&once);
        assert_eq!(once, twice, "fmt not idempotent on open_ballot.cov");
    }

    #[test]
    fn format_is_idempotent_shielded_counter() {
        let src =
            include_str!("../../../covenant-lexer/tests/fixtures/example_04_shielded_counter.cov");
        let once = fmt(src);
        let twice = fmt(&once);
        assert_eq!(once, twice, "fmt not idempotent on shielded_counter.cov");
    }

    #[test]
    fn format_is_idempotent_private_dao() {
        let src = include_str!("../../../covenant-lexer/tests/fixtures/example_07_private_dao.cov");
        let once = fmt(src);
        let twice = fmt(&once);
        assert_eq!(once, twice, "fmt not idempotent on private_dao.cov");
    }

    #[test]
    fn format_is_idempotent_safe_transfer() {
        let src =
            include_str!("../../../covenant-lexer/tests/fixtures/example_11_safe_transfer.cov");
        let once = fmt(src);
        let twice = fmt(&once);
        assert_eq!(once, twice, "fmt not idempotent on safe_transfer.cov");
    }

    #[test]
    fn format_preserves_field_types() {
        let src = "record R { balances: map<address, amount>\nfield owner: address }";
        let out = fmt(src);
        assert!(out.contains("map<address, amount>"));
        assert!(out.contains("field owner: address"));
    }

    #[test]
    fn format_error_on_invalid_source() {
        let result = format_source("this is not valid covenant", Path::new("bad.cov"));
        assert!(result.is_err(), "invalid source should fail formatting");
    }

    #[test]
    fn diff_empty_when_identical() {
        let d = make_diff("hello\nworld\n", "hello\nworld\n", "file.cov");
        assert!(d.is_empty(), "no diff when content is identical");
    }

    #[test]
    fn diff_nonempty_when_different() {
        let d = make_diff("hello\n", "world\n", "file.cov");
        assert!(
            !d.is_empty(),
            "diff should be non-empty for different content"
        );
        assert!(
            d.contains("---") && d.contains("+++"),
            "diff header expected"
        );
    }

    #[test]
    fn collect_cov_files_from_dir() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a.cov"), "record A {}").unwrap();
        std::fs::write(tmp.path().join("b.txt"), "ignored").unwrap();
        let files = collect_cov_files(tmp.path());
        assert_eq!(files.len(), 1, "only .cov files");
        assert!(files[0].file_name().unwrap() == "a.cov");
    }
}
