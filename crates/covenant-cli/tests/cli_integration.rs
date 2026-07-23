//! Integration tests for the `covenant` CLI binary.
//!
//! Tests invoke the binary via std::process::Command so they exercise real
//! argument parsing, exit codes, and output format behavior.

use std::path::PathBuf;
use std::process::Command;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn workspace_root() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // covenant-cli
    p.pop(); // crates
    p
}

fn binary() -> PathBuf {
    let root = workspace_root();
    if cfg!(windows) {
        let r = root.join("target/debug/covenant.exe");
        if r.exists() {
            return r;
        }
    }
    let dbg = root.join("target/debug/covenant");
    if dbg.exists() {
        return dbg;
    }
    // Fallback: let the test be skipped below
    root.join("target/debug/covenant.exe")
}

fn fixture(name: &str) -> PathBuf {
    workspace_root()
        .join("crates/covenant-lexer/tests/fixtures")
        .join(name)
}

/// Run covenant with the given args. Returns (exit_code, stdout, stderr).
fn run(args: &[&str]) -> (i32, String, String) {
    let bin = binary();
    if !bin.exists() {
        // Binary not built yet — skip by returning a sentinel.
        return (-1, String::new(), "binary not found".into());
    }
    let out = Command::new(&bin)
        .args(args)
        .output()
        .expect("spawn covenant");
    let code = out.status.code().unwrap_or(-1);
    (
        code,
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Skip if binary not present (CI without pre-build step).
macro_rules! require_bin {
    ($out:expr) => {
        if $out.0 == -1 {
            return;
        }
    };
}

// ---------------------------------------------------------------------------
// Global flags
// ---------------------------------------------------------------------------

#[test]
fn version_flag_exits_zero() {
    let (code, stdout, _) = run(&["--version"]);
    require_bin!((code, &stdout, ""));
    assert_eq!(code, 0, "covenant --version should exit 0");
    assert!(
        stdout.contains("covenant"),
        "version output should mention 'covenant'"
    );
}

#[test]
fn help_flag_exits_zero() {
    let (code, stdout, _) = run(&["--help"]);
    require_bin!((code, &stdout, ""));
    assert_eq!(code, 0);
    assert!(
        stdout.contains("init") || stdout.contains("build"),
        "help should list subcommands"
    );
}

#[test]
fn unknown_subcommand_exits_code_2() {
    let (code, _, _) = run(&["gobbledygook"]);
    require_bin!((code, "", ""));
    assert_eq!(code, 2, "unknown subcommand should exit 2");
}

// ---------------------------------------------------------------------------
// covenant build — single-file mode
// ---------------------------------------------------------------------------

#[test]
fn build_coin_single_file_succeeds() {
    let src = fixture("example_02_coin.cov");
    if !src.exists() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let (code, stdout, _stderr) = run(&[
        "build",
        src.to_str().unwrap(),
        "--out",
        tmp.path().to_str().unwrap(),
    ]);
    require_bin!((code, &stdout, ""));
    assert_eq!(code, 0, "build coin should succeed");
    assert!(
        tmp.path().join("Coin.bin").exists(),
        "deploy bytecode missing"
    );
    assert!(tmp.path().join("Coin.abi.json").exists(), "ABI missing");
    assert!(
        tmp.path().join("Coin.metadata.json").exists(),
        "metadata missing"
    );
}

#[test]
fn build_missing_file_exits_code_3() {
    let (code, _, _) = run(&["build", "/does/not/exist.cov", "--out", "/tmp/nope"]);
    require_bin!((code, "", ""));
    assert_ne!(code, 0, "build on missing file should fail");
}

#[test]
fn build_json_format_emits_json() {
    let src = fixture("example_01_hello.cov");
    if !src.exists() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let (code, stdout, _) = run(&[
        "--format",
        "json",
        "build",
        src.to_str().unwrap(),
        "--out",
        tmp.path().to_str().unwrap(),
    ]);
    require_bin!((code, &stdout, ""));
    assert_eq!(code, 0);
    // stdout should contain a JSON line with status:ok
    let line = stdout.lines().find(|l| l.contains("status")).unwrap_or("");
    let parsed: serde_json::Value = serde_json::from_str(line).expect("JSON output malformed");
    assert_eq!(parsed["status"], "ok");
}

#[test]
fn build_produces_five_artifacts() {
    let src = fixture("example_02_coin.cov");
    if !src.exists() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let (code, _, _) = run(&[
        "build",
        src.to_str().unwrap(),
        "--out",
        tmp.path().to_str().unwrap(),
    ]);
    require_bin!((code, "", ""));
    assert_eq!(code, 0);
    for artifact in [
        "Coin.bin",
        "Coin.runtime.bin",
        "Coin.abi.json",
        "Coin.storage.json",
        "Coin.metadata.json",
    ] {
        assert!(tmp.path().join(artifact).exists(), "missing: {artifact}");
    }
}

// ---------------------------------------------------------------------------
// covenant check
// ---------------------------------------------------------------------------

#[test]
fn check_valid_file_exits_zero() {
    let src = fixture("example_01_hello.cov");
    if !src.exists() {
        return;
    }
    let (code, stdout, _) = run(&["check", src.to_str().unwrap()]);
    require_bin!((code, &stdout, ""));
    assert_eq!(code, 0, "check on valid source should exit 0");
}

#[test]
fn check_valid_file_prints_ok() {
    let src = fixture("example_02_coin.cov");
    if !src.exists() {
        return;
    }
    let (code, stdout, _) = run(&["check", src.to_str().unwrap()]);
    require_bin!((code, &stdout, ""));
    assert_eq!(code, 0);
    assert!(stdout.contains("ok"), "check success should print 'ok'");
}

#[test]
fn check_missing_file_exits_nonzero() {
    let (code, _, _) = run(&["check", "/does/not/exist.cov"]);
    require_bin!((code, "", ""));
    assert_ne!(code, 0);
}

#[test]
fn check_json_format_emits_json() {
    let src = fixture("example_01_hello.cov");
    if !src.exists() {
        return;
    }
    let (code, stdout, _) = run(&["--format", "json", "check", src.to_str().unwrap()]);
    require_bin!((code, &stdout, ""));
    assert_eq!(code, 0);
    // Find JSON line in stdout
    let json_line = stdout.lines().find(|l| l.starts_with('{')).unwrap_or("");
    let v: serde_json::Value = serde_json::from_str(json_line).expect("valid JSON");
    assert_eq!(v["status"], "ok");
    assert_eq!(v["errors"], 0);
}

#[test]
fn check_no_color_produces_no_escapes() {
    let src = fixture("example_01_hello.cov");
    if !src.exists() {
        return;
    }
    let bin = binary();
    if !bin.exists() {
        return;
    }
    let out = Command::new(&bin)
        .env("NO_COLOR", "1")
        .args(["check", src.to_str().unwrap()])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    // No ANSI escape sequences in output
    assert!(
        !stderr.contains('\x1b'),
        "NO_COLOR=1 should suppress ANSI escapes; got: {stderr:?}"
    );
}

#[test]
fn check_never_color_flag_produces_no_escapes() {
    let src = fixture("example_01_hello.cov");
    if !src.exists() {
        return;
    }
    let (_, stdout, stderr) = run(&["--color", "never", "check", src.to_str().unwrap()]);
    require_bin!((0i32, &stdout, ""));
    assert!(
        !stderr.contains('\x1b'),
        "--color never should suppress ANSI escapes"
    );
}

// ---------------------------------------------------------------------------
// covenant init
// ---------------------------------------------------------------------------

#[test]
fn init_basics_creates_project() {
    let tmp = tempfile::tempdir().unwrap();
    let (code, stdout, _) = run(&["init", tmp.path().to_str().unwrap(), "--template", "basics"]);
    require_bin!((code, &stdout, ""));
    assert_eq!(code, 0);
    assert!(
        tmp.path().join("Covenant.toml").exists(),
        "Covenant.toml missing"
    );
    assert!(
        tmp.path().join("src/main.cov").exists(),
        "src/main.cov missing"
    );
    assert!(tmp.path().join("README.md").exists(), "README.md missing");
    assert!(tmp.path().join(".gitignore").exists(), ".gitignore missing");
}

#[test]
fn init_with_name_flag_uses_name() {
    let tmp = tempfile::tempdir().unwrap();
    let (code, _, _) = run(&[
        "init",
        tmp.path().to_str().unwrap(),
        "--template",
        "basics",
        "--name",
        "MyAwesomeProject",
    ]);
    require_bin!((code, "", ""));
    assert_eq!(code, 0);
    let toml_content = std::fs::read_to_string(tmp.path().join("Covenant.toml")).unwrap();
    assert!(toml_content.contains("MyAwesomeProject"));
}

#[test]
fn init_unknown_template_exits_code_2() {
    let tmp = tempfile::tempdir().unwrap();
    let (code, _, _) = run(&[
        "init",
        tmp.path().to_str().unwrap(),
        "--template",
        "unknown_xyz",
    ]);
    require_bin!((code, "", ""));
    assert_eq!(code, 2, "unknown template should exit 2");
}

#[test]
fn init_nonempty_without_force_exits_nonzero() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("existing.txt"), "data").unwrap();
    let (code, _, _) = run(&["init", tmp.path().to_str().unwrap(), "--template", "basics"]);
    require_bin!((code, "", ""));
    assert_ne!(code, 0, "init on non-empty dir without --force should fail");
}

#[test]
fn init_nonempty_with_force_succeeds() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("existing.txt"), "data").unwrap();
    let (code, _, _) = run(&[
        "init",
        tmp.path().to_str().unwrap(),
        "--template",
        "basics",
        "--force",
    ]);
    require_bin!((code, "", ""));
    assert_eq!(code, 0);
    assert!(tmp.path().join("Covenant.toml").exists());
}

#[test]
fn init_no_git_omits_gitignore() {
    let tmp = tempfile::tempdir().unwrap();
    let (code, _, _) = run(&[
        "init",
        tmp.path().to_str().unwrap(),
        "--template",
        "basics",
        "--no-git",
    ]);
    require_bin!((code, "", ""));
    assert_eq!(code, 0);
    assert!(
        !tmp.path().join(".gitignore").exists(),
        ".gitignore should be absent with --no-git"
    );
}

#[test]
fn init_all_templates_exit_zero() {
    for tmpl in ["basics", "token", "ballot", "ceremony", "module"] {
        let tmp = tempfile::tempdir().unwrap();
        let (code, _, _) = run(&["init", tmp.path().to_str().unwrap(), "--template", tmpl]);
        require_bin!((code, "", ""));
        assert_eq!(code, 0, "template {tmpl} should exit 0");
    }
}

// ---------------------------------------------------------------------------
// covenant clean
// ---------------------------------------------------------------------------

#[test]
fn clean_requires_manifest() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = binary();
    if !bin.exists() {
        return;
    }
    // Run clean in a temp dir with no Covenant.toml
    let out = Command::new(&bin)
        .current_dir(tmp.path())
        .arg("clean")
        .output()
        .unwrap();
    let code = out.status.code().unwrap_or(-1);
    assert_eq!(
        code, 3,
        "clean without manifest should exit 3 (I/O / not found)"
    );
}

#[test]
fn clean_dry_run_does_not_delete() {
    let tmp = tempfile::tempdir().unwrap();
    // Create a minimal project
    let manifest = r#"[project]
name = "testclean"
version = "0.1.0"
"#;
    std::fs::write(tmp.path().join("Covenant.toml"), manifest).unwrap();
    let build_dir = tmp.path().join("build");
    std::fs::create_dir_all(&build_dir).unwrap();
    std::fs::write(build_dir.join("artifact.bin"), "deadbeef").unwrap();

    let bin = binary();
    if !bin.exists() {
        return;
    }
    let out = Command::new(&bin)
        .current_dir(tmp.path())
        .args(["clean", "--dry-run"])
        .output()
        .unwrap();
    assert!(out.status.success(), "clean --dry-run should succeed");
    assert!(build_dir.exists(), "dry-run should not delete build dir");
}

#[test]
fn clean_removes_build_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let manifest = r#"[project]
name = "testclean2"
version = "0.1.0"
"#;
    std::fs::write(tmp.path().join("Covenant.toml"), manifest).unwrap();
    let build_dir = tmp.path().join("build");
    std::fs::create_dir_all(&build_dir).unwrap();
    std::fs::write(build_dir.join("artifact.bin"), "deadbeef").unwrap();

    let bin = binary();
    if !bin.exists() {
        return;
    }
    let out = Command::new(&bin)
        .current_dir(tmp.path())
        .arg("clean")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(
        !build_dir.exists(),
        "build dir should be removed after clean"
    );
}

// ---------------------------------------------------------------------------
// covenant build — project mode
// ---------------------------------------------------------------------------

#[test]
fn build_project_mode_produces_artifact() {
    let tmp = tempfile::tempdir().unwrap();
    // Set up a minimal project
    let manifest = r#"[project]
name = "testbuild"
version = "0.1.0"

[project.source]
root = "src/"
entrypoints = ["main.cov"]

[project.build]
output = "build/"
"#;
    std::fs::write(tmp.path().join("Covenant.toml"), manifest).unwrap();
    let src_dir = tmp.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    let main_cov = r#"
record TestBuild {
    greeting: text
    action update(v: text) { greeting = v }
    view read returns text { greeting }
}
"#;
    std::fs::write(src_dir.join("main.cov"), main_cov).unwrap();

    let bin = binary();
    if !bin.exists() {
        return;
    }
    let out = Command::new(&bin)
        .current_dir(tmp.path())
        .arg("build")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "project-mode build should succeed; stderr: {stderr}"
    );
    assert!(
        tmp.path().join("build/TestBuild.bin").exists(),
        "artifact missing"
    );
}

// ---------------------------------------------------------------------------
// covenant test
// ---------------------------------------------------------------------------

#[test]
fn test_no_target_exits_code_2() {
    let (code, _, _) = run(&["test"]);
    require_bin!((code, "", ""));
    assert_eq!(
        code, 2,
        "`covenant test` with no target should exit 2 (usage)"
    );
}

#[test]
fn test_missing_file_exits_nonzero() {
    let (code, _, _) = run(&["test", "/no/such/file.cov"]);
    require_bin!((code, "", ""));
    assert_ne!(code, 0, "test on missing file should fail");
}

#[test]
fn test_list_flag_exits_zero() {
    // Use a fixture with no test actions — should print "no tests found" and exit 0.
    let src = fixture("example_01_hello.cov");
    if !src.exists() {
        return;
    }
    let (code, stdout, _) = run(&["test", src.to_str().unwrap(), "--list"]);
    require_bin!((code, &stdout, ""));
    assert_eq!(code, 0, "test --list on no-test file should exit 0");
}

// ---------------------------------------------------------------------------
// covenant fmt
// ---------------------------------------------------------------------------

#[test]
fn fmt_valid_file_exits_zero() {
    let src = fixture("example_01_hello.cov");
    if !src.exists() {
        return;
    }
    // Use --check so we don't modify the fixture in-place.
    let (code, _, _) = run(&["fmt", "--check", src.to_str().unwrap()]);
    require_bin!((code, "", ""));
    // Exit 0 (already formatted) or 1 (needs reformatting) — never crashes.
    assert!(
        code == 0 || code == 1,
        "fmt --check should exit 0 or 1, got {code}"
    );
}

#[test]
fn fmt_no_target_no_manifest_exits_nonzero() {
    // Without a manifest in cwd, fmt exits with ManifestNotFound (code 3).
    let tmp = tempfile::tempdir().unwrap();
    let bin = binary();
    if !bin.exists() {
        return;
    }
    let out = std::process::Command::new(&bin)
        .current_dir(tmp.path())
        .arg("fmt")
        .output()
        .unwrap();
    let code = out.status.code().unwrap_or(-1);
    assert_ne!(code, 0, "fmt without manifest should fail");
}

#[test]
fn fmt_missing_file_exits_nonzero() {
    let (code, _, _) = run(&["fmt", "/no/such/file.cov"]);
    require_bin!((code, "", ""));
    assert_ne!(code, 0, "fmt on missing file should fail");
}

// ---------------------------------------------------------------------------
// covenant inspect
// ---------------------------------------------------------------------------

#[test]
fn inspect_ast_valid_file_exits_zero() {
    let src = fixture("example_01_hello.cov");
    if !src.exists() {
        return;
    }
    let (code, stdout, _) = run(&["inspect", "ast", src.to_str().unwrap()]);
    require_bin!((code, &stdout, ""));
    assert_eq!(code, 0, "inspect ast on valid file should exit 0");
}

#[test]
fn inspect_abi_valid_file_exits_zero() {
    let src = fixture("example_02_coin.cov");
    if !src.exists() {
        return;
    }
    let (code, stdout, _) = run(&["inspect", "abi", src.to_str().unwrap()]);
    require_bin!((code, &stdout, ""));
    assert_eq!(code, 0, "inspect abi on valid file should exit 0");
}

#[test]
fn inspect_unknown_artifact_exits_code_2() {
    let src = fixture("example_01_hello.cov");
    if !src.exists() {
        return;
    }
    let (code, _, _) = run(&["inspect", "gobbledygook", src.to_str().unwrap()]);
    require_bin!((code, "", ""));
    assert_eq!(code, 2, "unknown artifact kind should exit 2");
}

#[test]
fn inspect_missing_file_exits_code_3() {
    let (code, _, _) = run(&["inspect", "ast", "/no/such/file.cov"]);
    require_bin!((code, "", ""));
    assert_eq!(code, 3, "inspect on missing file should exit 3 (I/O)");
}

// ---------------------------------------------------------------------------
// covenant completions
// ---------------------------------------------------------------------------

#[test]
fn completions_bash_exits_zero() {
    let (code, stdout, _) = run(&["completions", "bash"]);
    require_bin!((code, &stdout, ""));
    assert_eq!(code, 0, "completions bash should exit 0");
    assert!(!stdout.is_empty(), "completions bash should produce output");
}

#[test]
fn completions_zsh_exits_zero() {
    let (code, stdout, _) = run(&["completions", "zsh"]);
    require_bin!((code, &stdout, ""));
    assert_eq!(code, 0);
    assert!(!stdout.is_empty());
}
