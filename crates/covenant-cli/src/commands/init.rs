//! `covenant init`: scaffold a new Covenant project from a template.
//!
//! Templates are embedded as string constants. Available templates:
//! basics, token, ballot, ceremony, module.

use std::path::PathBuf;

use clap::Args;

use crate::error::CliError;

// ---------------------------------------------------------------------------
// Argument struct
// ---------------------------------------------------------------------------

#[derive(Args, Debug, Clone)]
pub struct InitArgs {
    /// Directory in which to create the project (default: current directory).
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Project name (defaults to directory name).
    #[arg(long)]
    pub name: Option<String>,

    /// Template: basics (default), token, ballot, ceremony, module.
    #[arg(long, default_value = "basics")]
    pub template: String,

    /// Proceed even if the target directory is non-empty.
    #[arg(long)]
    pub force: bool,

    /// Skip creating a .gitignore file.
    #[arg(long)]
    pub no_git: bool,
}

// ---------------------------------------------------------------------------
// Template definitions
// ---------------------------------------------------------------------------

struct TemplateFiles {
    covenant_toml: &'static str,
    main_cov: &'static str,
    readme: &'static str,
}

const GITIGNORE_CONTENT: &str = "build/\ntarget/\n.covenant-cache/\n";

fn template_for(name: &str) -> Option<TemplateFiles> {
    match name {
        "basics" => Some(TemplateFiles {
            covenant_toml: BASICS_TOML,
            main_cov: BASICS_COV,
            readme: BASICS_README,
        }),
        "token" => Some(TemplateFiles {
            covenant_toml: TOKEN_TOML,
            main_cov: TOKEN_COV,
            readme: TOKEN_README,
        }),
        "ballot" => Some(TemplateFiles {
            covenant_toml: BALLOT_TOML,
            main_cov: BALLOT_COV,
            readme: BALLOT_README,
        }),
        "ceremony" => Some(TemplateFiles {
            covenant_toml: CEREMONY_TOML,
            main_cov: CEREMONY_COV,
            readme: CEREMONY_README,
        }),
        "module" => Some(TemplateFiles {
            covenant_toml: MODULE_TOML,
            main_cov: MODULE_COV,
            readme: MODULE_README,
        }),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Template: basics
// ---------------------------------------------------------------------------

const BASICS_TOML: &str = r#"[project]
name = "{{project_name}}"
version = "0.1.0"

[project.source]
root = "src/"
entrypoints = ["main.cov"]

[project.build]
output = "build/"
"#;

const BASICS_COV: &str = r#"-- {{project_name}}: minimal Covenant module.

module {{project_name}} {
    field counter: amount

    action increment() {
        counter += 1
    }

    view get_count() returns amount {
        counter
    }
}
"#;

const BASICS_README: &str = r#"# {{project_name}}

A Covenant project scaffolded from the `basics` template.

## Build

```
covenant build
```

## Check

```
covenant check
```
"#;

// ---------------------------------------------------------------------------
// Template: token
// ---------------------------------------------------------------------------

const TOKEN_TOML: &str = r#"[project]
name = "{{project_name}}"
version = "0.1.0"

[project.source]
root = "src/"
entrypoints = ["main.cov"]

[project.build]
output = "build/"
"#;

const TOKEN_COV: &str = r#"-- {{project_name}}: ERC-20-compatible token.

token {{project_name}} {
    field supply: amount
    field balances: map<address, amount>

    action initialize(initial_supply: amount) {
        supply = initial_supply
        balances[caller] = initial_supply
    }
}
"#;

const TOKEN_README: &str = r#"# {{project_name}}

A Covenant ERC-20-compatible token project.

## Build

```
covenant build
```
"#;

// ---------------------------------------------------------------------------
// Template: ballot
// ---------------------------------------------------------------------------

const BALLOT_TOML: &str = r#"[project]
name = "{{project_name}}"
version = "0.1.0"

[project.source]
root = "src/"
entrypoints = ["main.cov"]

[project.build]
output = "build/"
"#;

const BALLOT_COV: &str = r#"-- {{project_name}}: open voting ballot.

ballot {{project_name}} {
    field tally: map<text, amount>
    field voted: map<address, bool>

    action vote(proposal: text) when !voted[caller] {
        tally[proposal] += 1
        voted[caller] = true
    }

    view votes_for(proposal: text) returns amount {
        tally[proposal]
    }
}
"#;

const BALLOT_README: &str = r#"# {{project_name}}

A Covenant voting ballot project.

## Build

```
covenant build
```
"#;

// ---------------------------------------------------------------------------
// Template: ceremony
// ---------------------------------------------------------------------------

const CEREMONY_TOML: &str = r#"[project]
name = "{{project_name}}"
version = "0.1.0"

[project.source]
root = "src/"
entrypoints = ["main.cov"]

[project.build]
output = "build/"
"#;

const CEREMONY_COV: &str = r#"-- {{project_name}}: ERC-8228 amnesia ceremony.

ceremony {{project_name}}
    guardians: 3
    threshold: 2
{
    on_destroy {
        destroy(0)
    }
}
"#;

const CEREMONY_README: &str = r#"# {{project_name}}

A Covenant ERC-8228 amnesia ceremony project.

## Build

```
covenant build
```
"#;

// ---------------------------------------------------------------------------
// Template: module
// ---------------------------------------------------------------------------

const MODULE_TOML: &str = r#"[project]
name = "{{project_name}}"
version = "0.1.0"

[project.source]
root = "src/"
entrypoints = ["main.cov"]

[project.build]
output = "build/"
"#;

const MODULE_COV: &str = r#"-- {{project_name}}: bare module construct.

module {{project_name}} {
    field value: amount

    action set(v: amount) {
        value = v
    }

    view get() returns amount {
        value
    }
}
"#;

const MODULE_README: &str = r#"# {{project_name}}

A Covenant module project.

## Build

```
covenant build
```
"#;

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

pub fn run(args: &InitArgs) -> Result<(), CliError> {
    let tmpl = template_for(&args.template)
        .ok_or_else(|| CliError::UnknownTemplate(args.template.clone()))?;

    // Resolve destination path
    let dest = if args.path.as_os_str() == "." {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    } else {
        args.path.clone()
    };

    // Create directory if needed
    if !dest.exists() {
        std::fs::create_dir_all(&dest)?;
    }

    // Check emptiness unless --force
    if !args.force {
        let is_empty = std::fs::read_dir(&dest)
            .map(|mut d| d.next().is_none())
            .unwrap_or(true);
        if !is_empty {
            return Err(CliError::NonEmptyTarget(dest.display().to_string()));
        }
    }

    // Derive project name
    let project_name = args.name.clone().unwrap_or_else(|| {
        dest.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "covenant-project".to_string())
    });

    let substitute = |s: &str| s.replace("{{project_name}}", &project_name);

    // Write Covenant.toml
    write_text(&dest.join("Covenant.toml"), &substitute(tmpl.covenant_toml))?;

    // Write src/main.cov
    let src_dir = dest.join("src");
    std::fs::create_dir_all(&src_dir)?;
    write_text(&src_dir.join("main.cov"), &substitute(tmpl.main_cov))?;

    // Write README.md
    write_text(&dest.join("README.md"), &substitute(tmpl.readme))?;

    // Write .gitignore (unless --no-git)
    if !args.no_git {
        write_text(&dest.join(".gitignore"), GITIGNORE_CONTENT)?;
    }

    println!(
        "Created Covenant project `{}` at {}",
        project_name,
        dest.display()
    );
    Ok(())
}

fn write_text(path: &std::path::Path, content: &str) -> Result<(), CliError> {
    std::fs::write(path, content).map_err(|e| {
        CliError::Io(std::io::Error::new(
            e.kind(),
            format!("cannot write `{}`: {e}", path.display()),
        ))
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_args(path: PathBuf, template: &str) -> InitArgs {
        InitArgs {
            path,
            name: None,
            template: template.to_string(),
            force: true,
            no_git: false,
        }
    }

    #[test]
    fn init_basics_creates_expected_files() {
        let tmp = tempfile::tempdir().unwrap();
        let args = make_args(tmp.path().to_path_buf(), "basics");
        run(&args).unwrap();

        assert!(tmp.path().join("Covenant.toml").exists());
        assert!(tmp.path().join("src/main.cov").exists());
        assert!(tmp.path().join("README.md").exists());
        assert!(tmp.path().join(".gitignore").exists());
    }

    #[test]
    fn init_substitutes_project_name() {
        let tmp = tempfile::tempdir().unwrap();
        let mut args = make_args(tmp.path().to_path_buf(), "basics");
        args.name = Some("MyProject".to_string());
        run(&args).unwrap();

        let toml = std::fs::read_to_string(tmp.path().join("Covenant.toml")).unwrap();
        assert!(
            toml.contains("MyProject"),
            "project name not substituted in TOML"
        );

        let cov = std::fs::read_to_string(tmp.path().join("src/main.cov")).unwrap();
        assert!(
            cov.contains("MyProject"),
            "project name not substituted in .cov"
        );
    }

    #[test]
    fn init_no_git_omits_gitignore() {
        let tmp = tempfile::tempdir().unwrap();
        let mut args = make_args(tmp.path().to_path_buf(), "basics");
        args.no_git = true;
        run(&args).unwrap();

        assert!(!tmp.path().join(".gitignore").exists());
    }

    #[test]
    fn init_unknown_template_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let args = make_args(tmp.path().to_path_buf(), "nonexistent");
        let result = run(&args);
        assert!(matches!(result, Err(CliError::UnknownTemplate(_))));
    }

    #[test]
    fn init_nonempty_without_force_errors() {
        let tmp = tempfile::tempdir().unwrap();
        // Place a file in the directory
        std::fs::write(tmp.path().join("existing.txt"), "data").unwrap();
        let mut args = make_args(tmp.path().to_path_buf(), "basics");
        args.force = false;
        let result = run(&args);
        assert!(matches!(result, Err(CliError::NonEmptyTarget(_))));
    }

    #[test]
    fn init_nonempty_with_force_succeeds() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("existing.txt"), "data").unwrap();
        let args = make_args(tmp.path().to_path_buf(), "basics");
        run(&args).unwrap();
        assert!(tmp.path().join("Covenant.toml").exists());
    }

    #[test]
    fn init_all_templates_succeed() {
        for tmpl in ["basics", "token", "ballot", "ceremony", "module"] {
            let tmp = tempfile::tempdir().unwrap();
            let args = make_args(tmp.path().to_path_buf(), tmpl);
            run(&args).unwrap_or_else(|e| panic!("template {tmpl} failed: {e}"));
            assert!(
                tmp.path().join("Covenant.toml").exists(),
                "template {tmpl} missing Covenant.toml"
            );
        }
    }

    #[test]
    fn init_creates_directory_if_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let new_dir = tmp.path().join("new_project");
        assert!(!new_dir.exists());
        let args = make_args(new_dir.clone(), "module");
        run(&args).unwrap();
        assert!(new_dir.join("Covenant.toml").exists());
    }
}
