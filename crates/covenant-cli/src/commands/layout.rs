//! `covenant layout diff <prev.json> <curr.json>`: compare two storage
//! layout sidecars and report removed, reordered, or type-changed fields.
//!
//! KSR-CVN-038: the compiler already emits `layout.json` as a build sidecar
//! (see `build::serialize_storage_layout`). This subcommand gives CI a way
//! to fail an upgrade PR when the layout changes in a storage-incompatible
//! direction (field removal, slot move, type change).

use std::path::PathBuf;

use clap::{Args, Subcommand};
use serde_json::Value;

use crate::error::CliError;

#[derive(Args, Debug, Clone)]
pub struct LayoutArgs {
    #[command(subcommand)]
    pub command: LayoutCommand,
}

#[derive(Subcommand, Debug, Clone)]
pub enum LayoutCommand {
    /// Diff two storage-layout sidecars. Exits non-zero on breaking changes.
    Diff(DiffArgs),
}

#[derive(Args, Debug, Clone)]
pub struct DiffArgs {
    /// Path to the previous `layout.json`.
    pub prev: PathBuf,
    /// Path to the current `layout.json`.
    pub curr: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    name: String,
    slot: String,
    ty: String,
}

fn parse_layout(path: &PathBuf) -> Result<Vec<Entry>, CliError> {
    let raw = std::fs::read_to_string(path)?;
    let v: Value = serde_json::from_str(&raw)
        .map_err(|e| CliError::Usage(format!("`{}` is not valid JSON: {e}", path.display())))?;
    let arr = v.get("entries").and_then(|x| x.as_array()).ok_or_else(|| {
        CliError::Usage(format!(
            "`{}` has no `entries` array (is this a layout.json?)",
            path.display()
        ))
    })?;
    let mut out = Vec::with_capacity(arr.len());
    for e in arr {
        let name = e
            .get("name")
            .and_then(|x| x.as_str())
            .ok_or_else(|| CliError::Usage("entry missing `name`".into()))?
            .to_string();
        let slot = e
            .get("slot")
            .and_then(|x| x.as_str())
            .ok_or_else(|| CliError::Usage("entry missing `slot`".into()))?
            .to_string();
        let ty = e
            .get("type")
            .and_then(|x| x.as_str())
            .ok_or_else(|| CliError::Usage("entry missing `type`".into()))?
            .to_string();
        out.push(Entry { name, slot, ty });
    }
    Ok(out)
}

pub fn run(args: &LayoutArgs) -> Result<(), CliError> {
    match &args.command {
        LayoutCommand::Diff(d) => run_diff(d),
    }
}

fn run_diff(args: &DiffArgs) -> Result<(), CliError> {
    let prev = parse_layout(&args.prev)?;
    let curr = parse_layout(&args.curr)?;

    let mut breaking: Vec<String> = Vec::new();
    let mut notes: Vec<String> = Vec::new();

    // Removed or moved: every prev entry must still be at the same slot and
    // type in curr. Addition at a new slot is OK; reuse of an old slot for a
    // new field is breaking.
    for p in &prev {
        match curr.iter().find(|c| c.name == p.name) {
            None => {
                breaking.push(format!(
                    "removed: `{}` (was slot {}, {})",
                    p.name, p.slot, p.ty
                ));
            }
            Some(c) if c.slot != p.slot => {
                breaking.push(format!(
                    "moved:   `{}`: slot {} → {}",
                    p.name, p.slot, c.slot
                ));
            }
            Some(c) if c.ty != p.ty => {
                breaking.push(format!(
                    "retyped: `{}` at slot {}: {} → {}",
                    p.name, p.slot, p.ty, c.ty
                ));
            }
            Some(_) => {}
        }
    }

    // Slot reuse for a different name (curr field at prev's slot with different name).
    for c in &curr {
        if prev.iter().any(|p| p.name == c.name) {
            continue;
        }
        if let Some(p) = prev.iter().find(|p| p.slot == c.slot) {
            breaking.push(format!(
                "slot-reuse: slot {} was `{}`, now `{}`",
                c.slot, p.name, c.name
            ));
        } else {
            notes.push(format!(
                "added:   `{}` at slot {} ({})",
                c.name, c.slot, c.ty
            ));
        }
    }

    for n in &notes {
        println!("{n}");
    }
    for b in &breaking {
        println!("BREAKING: {b}");
    }

    if !breaking.is_empty() {
        return Err(CliError::Usage(format!(
            "{} breaking storage-layout change(s) detected",
            breaking.len()
        )));
    }
    println!("no breaking storage-layout changes");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_tmp(content: &str, name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("cov_layout_test_{name}.json"));
        std::fs::write(&p, content).unwrap();
        p
    }

    #[test]
    fn identical_layouts_are_clean() {
        let j = r#"{ "entries": [ { "name": "a", "slot": "0x00", "offset": 0, "size": 32, "type": "uint256" } ] }"#;
        let prev = write_tmp(j, "id_a");
        let curr = write_tmp(j, "id_b");
        let r = run_diff(&DiffArgs { prev, curr });
        assert!(r.is_ok(), "identical layouts must pass");
    }

    #[test]
    fn removing_a_field_is_breaking() {
        let a = r#"{ "entries": [ { "name": "a", "slot": "0x00", "offset": 0, "size": 32, "type": "uint256" } ] }"#;
        let b = r#"{ "entries": [] }"#;
        let prev = write_tmp(a, "rm_a");
        let curr = write_tmp(b, "rm_b");
        let r = run_diff(&DiffArgs { prev, curr });
        assert!(r.is_err(), "removing a field must be breaking");
    }

    #[test]
    fn changing_a_type_is_breaking() {
        let a = r#"{ "entries": [ { "name": "a", "slot": "0x00", "offset": 0, "size": 32, "type": "uint256" } ] }"#;
        let b = r#"{ "entries": [ { "name": "a", "slot": "0x00", "offset": 0, "size": 32, "type": "address" } ] }"#;
        let prev = write_tmp(a, "ty_a");
        let curr = write_tmp(b, "ty_b");
        let r = run_diff(&DiffArgs { prev, curr });
        assert!(r.is_err(), "type change must be breaking");
    }

    #[test]
    fn moving_a_field_is_breaking() {
        let a = r#"{ "entries": [ { "name": "a", "slot": "0x00", "offset": 0, "size": 32, "type": "uint256" } ] }"#;
        let b = r#"{ "entries": [ { "name": "a", "slot": "0x01", "offset": 0, "size": 32, "type": "uint256" } ] }"#;
        let prev = write_tmp(a, "mv_a");
        let curr = write_tmp(b, "mv_b");
        let r = run_diff(&DiffArgs { prev, curr });
        assert!(r.is_err(), "slot move must be breaking");
    }

    #[test]
    fn adding_a_field_is_ok() {
        let a = r#"{ "entries": [ { "name": "a", "slot": "0x00", "offset": 0, "size": 32, "type": "uint256" } ] }"#;
        let b = r#"{ "entries": [
            { "name": "a", "slot": "0x00", "offset": 0, "size": 32, "type": "uint256" },
            { "name": "b", "slot": "0x01", "offset": 0, "size": 32, "type": "address" }
        ] }"#;
        let prev = write_tmp(a, "add_a");
        let curr = write_tmp(b, "add_b");
        let r = run_diff(&DiffArgs { prev, curr });
        assert!(r.is_ok(), "adding a new field at a fresh slot is safe");
    }

    #[test]
    fn slot_reuse_with_different_name_is_breaking() {
        let a = r#"{ "entries": [ { "name": "a", "slot": "0x00", "offset": 0, "size": 32, "type": "uint256" } ] }"#;
        let b = r#"{ "entries": [ { "name": "c", "slot": "0x00", "offset": 0, "size": 32, "type": "uint256" } ] }"#;
        let prev = write_tmp(a, "reuse_a");
        let curr = write_tmp(b, "reuse_b");
        let r = run_diff(&DiffArgs { prev, curr });
        assert!(
            r.is_err(),
            "reusing a slot for a different field is breaking"
        );
    }
}
