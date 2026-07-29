//! `covenant test`: run inline test blocks via the MockChain harness.
//!
//! Discovers actions with names starting with `test_` (or annotated `@test`)
//! in the source file, compiles and deploys the construct, then calls each
//! test action and reports pass / fail.

use std::path::PathBuf;

use clap::Args;
use covenant_diag::SourceId;
use covenant_testing::{CallResult, CovenantTestHarness};

use crate::error::CliError;

#[derive(Args, Debug, Clone)]
pub struct TestArgs {
    /// Source file containing the test construct.
    #[arg(value_name = "TARGET")]
    pub target: Option<PathBuf>,

    /// Run only tests whose name matches this pattern (case-insensitive substring).
    #[arg(value_name = "FILTER")]
    pub filter: Option<String>,

    /// Continue running after the first failure.
    #[arg(long)]
    pub no_fail_fast: bool,

    /// List all discovered tests without running them.
    #[arg(long)]
    pub list: bool,

    /// Print a gas usage report after each test (stub, not yet metered).
    #[arg(long)]
    pub gas_report: bool,

    /// Print an action-level coverage report after the test run.
    ///
    /// V0.9.0: name-heuristic coverage : for every non-test action in the
    /// contract, check whether at least one `test_*` action's name contains
    /// the action name. Reports covered / uncovered counts and lists the
    /// uncovered actions. V0.9.x will replace this with IR-instrumented
    /// per-action execution tracking.
    #[arg(long)]
    pub coverage: bool,
}

pub fn run(args: TestArgs, verbose: u8) -> Result<(), CliError> {
    let target = args
        .target
        .as_ref()
        .ok_or_else(|| CliError::Usage("a target .cov source file is required".to_string()))?;

    let source = std::fs::read_to_string(target).map_err(|e| {
        CliError::Io(std::io::Error::new(
            e.kind(),
            format!("cannot read `{}`: {e}", target.display()),
        ))
    })?;

    // Discover test actions from the AST.
    let test_sigs = discover_tests(&source, args.filter.as_deref());

    if test_sigs.is_empty() {
        println!("no tests found");
        return Ok(());
    }

    if args.list {
        for sig in &test_sigs {
            println!("{sig}");
        }
        return Ok(());
    }

    // V0.9 Sprint 40: test isolation.
    //
    // Each test runs against a FRESH harness (= fresh MockChain instance,
    // fresh contract deploy). This guarantees state set by `test_a` cannot
    // leak into `test_b`. The cost is one redeploy per test (~50ms compile
    // cache + deploy); for typical 10-20 tests per file this is sub-second
    // and worth the determinism.
    //
    // Why per-test fresh harness instead of MockChain snapshot/revert :
    // CovenantTestHarness doesn't yet expose snapshot/revert (V1.0 backlog
    //: adding it requires plumbing through the WASM runtime). Fresh
    // harness is the V0.9.0 pragmatic alternative; semantically equivalent
    // for test isolation.

    let mut pass = 0usize;
    let mut fail = 0usize;

    for sig in &test_sigs {
        // Fresh harness: fresh world for each test.
        let mut harness = CovenantTestHarness::new();
        let deployer = harness.addrs.deployer;
        let contract = match harness.deploy(&source, deployer) {
            Ok(c) => c,
            Err(diags) => {
                return Err(CliError::Usage(format!(
                    "compilation failed ({} error(s))",
                    diags.len()
                )));
            }
        };

        let result = harness.call(contract, deployer, sig, &[]);
        match result {
            CallResult::Ok(_) => {
                println!("test {sig} ... ok");
                if args.gas_report {
                    println!("  (gas reporting not yet metered)");
                }
                pass += 1;
            }
            CallResult::Revert(data) => {
                println!("test {sig} ... FAILED");
                if verbose >= 1 && !data.is_empty() {
                    println!("  revert data: 0x{}", hex::encode(&data));
                }
                fail += 1;
                if !args.no_fail_fast {
                    break;
                }
            }
            CallResult::Abort(reason) => {
                println!("test {sig} ... FAILED (abort: {reason})");
                fail += 1;
                if !args.no_fail_fast {
                    break;
                }
            }
        }
    }

    println!("\ntest result: {pass} passed; {fail} failed");

    if args.coverage {
        emit_coverage_report(&source);
    }

    if fail > 0 {
        Err(CliError::TestFailed(fail))
    } else {
        Ok(())
    }
}

/// V0.9.0: name-heuristic action coverage.
///
/// For every non-test action declared in the contract, check whether at
/// least one `test_*` action's name contains it (case-insensitive substring).
/// Emit a count + list of uncovered actions.
///
/// Limitations :
///   - Pure heuristic. `test_foo` "covers" action `foo` even if its body
///     never actually calls `foo`. V0.9.x will replace with IR
///     instrumentation that records executed action selectors per test.
///   - View functions are not counted (they're typically tested implicitly
///     via the `when` guard pattern of zero-arg test actions).
fn emit_coverage_report(source: &str) {
    let (test_names, action_names) = discover_all_actions(source);

    if action_names.is_empty() {
        println!("\ncoverage: no non-test actions in contract");
        return;
    }

    let test_names_lc: Vec<String> = test_names.iter().map(|n| n.to_lowercase()).collect();

    let mut covered = Vec::new();
    let mut uncovered = Vec::new();
    for name in &action_names {
        let n_lc = name.to_lowercase();
        let hit = test_names_lc.iter().any(|t| t.contains(&n_lc));
        if hit {
            covered.push(name.clone());
        } else {
            uncovered.push(name.clone());
        }
    }

    let total = action_names.len();
    let pct = (covered.len() as f64 / total as f64) * 100.0;
    println!(
        "\ncoverage: {} / {} actions covered ({:.0}%): name-heuristic",
        covered.len(),
        total,
        pct
    );
    if !uncovered.is_empty() {
        println!("uncovered actions:");
        for name in &uncovered {
            println!("  - {name}");
        }
    }
}

/// Returns (test_action_names, non_test_action_names).
fn discover_all_actions(source: &str) -> (Vec<String>, Vec<String>) {
    let sid = SourceId::new(0);
    let (tokens, _) = covenant_lexer::tokenize(source, sid);
    let (file_opt, _) = covenant_parser::parse(&tokens, sid);

    let mut tests = Vec::new();
    let mut others = Vec::new();
    if let Some(file) = file_opt {
        for decl in &file.top_level.body {
            if let covenant_parser::ast::TopLevelDecl::Action(action) = decl {
                let name = action.name.name.as_ref().to_string();
                let is_test = name.starts_with("test_")
                    || action
                        .annotations
                        .iter()
                        .any(|a| a.name.name.as_ref() == "test");
                if is_test {
                    tests.push(name);
                } else {
                    others.push(name);
                }
            }
        }
    }
    (tests, others)
}

/// Parse the source and collect ABI signatures for zero-arg test actions.
///
/// Test actions are those whose names start with `test_` or that carry a
/// `@test` annotation. Only no-arg actions are included.
fn discover_tests(source: &str, filter: Option<&str>) -> Vec<String> {
    let sid = SourceId::new(0);
    let (tokens, _) = covenant_lexer::tokenize(source, sid);
    let (file_opt, _) = covenant_parser::parse(&tokens, sid);

    let mut sigs = Vec::new();
    if let Some(file) = file_opt {
        for decl in &file.top_level.body {
            if let covenant_parser::ast::TopLevelDecl::Action(action) = decl {
                let name = action.name.name.as_ref();
                let is_test = name.starts_with("test_")
                    || action
                        .annotations
                        .iter()
                        .any(|a| a.name.name.as_ref() == "test");
                if !is_test || !action.args.is_empty() {
                    continue;
                }
                if let Some(f) = filter {
                    if !name.to_lowercase().contains(&f.to_lowercase()) {
                        continue;
                    }
                }
                sigs.push(format!("{name}()"));
            }
        }
    }
    sigs
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const PASSING_TESTS: &str = r#"
record TestContract {
    x: amount

    action test_sets_x() only caller {
        x = 42
    }

    action test_increments() only caller {
        x += 1
    }

    action helper_not_a_test() only caller {
        x = 0
    }
}
"#;

    #[allow(dead_code)]
    const FAILING_TEST: &str = r#"
record TestFail {
    action test_always_fails() only caller {
        revert_with Unauthorized(caller)
    }

    error Unauthorized(who: address)
}
"#;

    #[test]
    fn discover_finds_test_prefix_actions() {
        let sigs = discover_tests(PASSING_TESTS, None);
        assert_eq!(sigs.len(), 2, "should find 2 test actions: {sigs:?}");
        assert!(sigs.contains(&"test_sets_x()".to_string()));
        assert!(sigs.contains(&"test_increments()".to_string()));
    }

    #[test]
    fn discover_respects_filter() {
        let sigs = discover_tests(PASSING_TESTS, Some("sets"));
        assert_eq!(sigs.len(), 1);
        assert_eq!(sigs[0], "test_sets_x()");
    }

    #[test]
    fn discover_skips_parameterized_actions() {
        let src = r#"
record R {
    action test_with_args(x: amount) only caller { }
    action test_no_args() only caller { }
}
"#;
        let sigs = discover_tests(src, None);
        assert_eq!(
            sigs.len(),
            1,
            "parameterized test actions should be skipped"
        );
        assert_eq!(sigs[0], "test_no_args()");
    }

    #[test]
    fn discover_empty_when_no_tests() {
        let src = r#"
record R {
    x: amount
    action go() only caller { x = 1 }
}
"#;
        let sigs = discover_tests(src, None);
        assert!(sigs.is_empty());
    }

    #[test]
    fn run_no_target_returns_usage_error() {
        let args = TestArgs {
            target: None,
            filter: None,
            no_fail_fast: false,
            list: false,
            gas_report: false,
            coverage: false,
        };
        let result = run(args, 0);
        assert!(matches!(result, Err(CliError::Usage(_))));
    }

    #[test]
    fn run_missing_file_returns_io_error() {
        let args = TestArgs {
            target: Some(PathBuf::from("/no/such/file.cov")),
            filter: None,
            no_fail_fast: false,
            list: false,
            gas_report: false,
            coverage: false,
        };
        let result = run(args, 0);
        assert!(matches!(result, Err(CliError::Io(_))));
    }

    #[test]
    fn run_passing_tests_returns_ok() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), PASSING_TESTS).unwrap();
        let args = TestArgs {
            target: Some(tmp.path().to_path_buf()),
            filter: None,
            no_fail_fast: false,
            list: false,
            gas_report: false,
            coverage: false,
        };
        assert!(run(args, 0).is_ok(), "passing tests should return Ok");
    }

    #[test]
    fn run_list_only_does_not_run_tests() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), PASSING_TESTS).unwrap();
        let args = TestArgs {
            target: Some(tmp.path().to_path_buf()),
            filter: None,
            no_fail_fast: false,
            list: true,
            gas_report: false,
            coverage: false,
        };
        // --list should succeed without deploying the contract
        assert!(run(args, 0).is_ok());
    }

    #[test]
    fn run_no_tests_found_returns_ok() {
        let src = r#"record R { x: amount }"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), src).unwrap();
        let args = TestArgs {
            target: Some(tmp.path().to_path_buf()),
            filter: None,
            no_fail_fast: false,
            list: false,
            gas_report: false,
            coverage: false,
        };
        assert!(run(args, 0).is_ok());
    }
}
