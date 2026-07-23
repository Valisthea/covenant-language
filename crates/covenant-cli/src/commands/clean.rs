//! `covenant clean` — remove build artifacts.

use std::path::PathBuf;

use clap::Args;

use crate::error::CliError;

#[derive(Args, Debug, Clone)]
pub struct CleanArgs {
    /// Only clean artifacts for a specific backend (evm, aster).
    #[arg(long)]
    pub target: Option<String>,

    /// Print what would be removed without removing it.
    #[arg(long)]
    pub dry_run: bool,
}

pub fn run(args: &CleanArgs, manifest_path: Option<&PathBuf>) -> Result<(), CliError> {
    let manifest_file = manifest_path
        .cloned()
        .or_else(|| covenant_manifest::Manifest::find_upward(&std::env::current_dir().unwrap()))
        .ok_or(CliError::ManifestNotFound)?;

    let manifest = covenant_manifest::Manifest::read(&manifest_file)?;
    let project_root = manifest_file.parent().unwrap().to_path_buf();

    let out_dir = project_root.join(
        manifest
            .project
            .build
            .output
            .clone()
            .unwrap_or_else(|| PathBuf::from("build")),
    );

    if !out_dir.exists() {
        println!("Nothing to clean.");
        return Ok(());
    }

    if args.dry_run {
        println!("Would remove: {}", out_dir.display());
        // List contents for clarity
        if let Ok(entries) = std::fs::read_dir(&out_dir) {
            for entry in entries.flatten() {
                println!("  {}", entry.path().display());
            }
        }
    } else {
        std::fs::remove_dir_all(&out_dir)?;
        println!("Removed: {}", out_dir.display());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_run_on_nonexistent_dir() {
        let tmp = std::env::temp_dir().join("covenant_clean_test_nonexistent");
        // Create a fake manifest in a temp dir
        let project_dir = std::env::temp_dir().join("covenant_clean_proj");
        let _ = std::fs::create_dir_all(&project_dir);
        let manifest_content = r#"
[project]
name = "testproject"
version = "0.1.0"
"#;
        std::fs::write(project_dir.join("Covenant.toml"), manifest_content).unwrap();
        let manifest_path = project_dir.join("Covenant.toml");

        let args = CleanArgs {
            target: None,
            dry_run: true,
        };
        // With default output dir "build/", which doesn't exist
        let result = run(&args, Some(&manifest_path));
        assert!(result.is_ok(), "clean on nonexistent dir should succeed");

        let _ = std::fs::remove_dir_all(&project_dir);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn removes_build_dir() {
        let project_dir = std::env::temp_dir().join("covenant_clean_proj2");
        let build_dir = project_dir.join("build");
        let _ = std::fs::create_dir_all(&build_dir);
        std::fs::write(build_dir.join("Coin.bin"), "deadbeef").unwrap();
        let manifest_content = r#"
[project]
name = "testproject"
version = "0.1.0"
"#;
        std::fs::write(project_dir.join("Covenant.toml"), manifest_content).unwrap();
        let manifest_path = project_dir.join("Covenant.toml");

        let args = CleanArgs {
            target: None,
            dry_run: false,
        };
        run(&args, Some(&manifest_path)).unwrap();
        assert!(!build_dir.exists(), "build dir should be removed");

        let _ = std::fs::remove_dir_all(&project_dir);
    }
}
