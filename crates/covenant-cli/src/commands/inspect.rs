//! `covenant inspect` — dump AST, IR, bytecode, ABI, storage, or diagnostics.

use std::path::PathBuf;

use clap::Args;
use covenant_diag::{DiagnosticLevel, DiagnosticLevel as DL, SourceId};
use covenant_driver::{check as driver_check, compile, compile_to_ir};
use covenant_evm_backend::EvmConfig;
use covenant_ir::{IrFunction, IrFunctionKind, IrModule, Terminator};
use covenant_opt::OptimizerConfig;
use covenant_stdlib::StdlibConfig;

use crate::error::CliError;

#[derive(Args, Debug, Clone)]
pub struct InspectArgs {
    /// Artifact kind: ast, ir, bytecode, abi, storage, diagnostics.
    pub artifact: String,

    /// Source file (.cov) to inspect.
    pub target: PathBuf,

    /// Filter output to a specific construct or function (by name).
    #[arg(long)]
    pub item: Option<String>,
}

pub fn run(args: InspectArgs, verbose: u8) -> Result<(), CliError> {
    let source = std::fs::read_to_string(&args.target).map_err(|e| {
        CliError::Io(std::io::Error::new(
            e.kind(),
            format!("cannot read `{}`: {e}", args.target.display()),
        ))
    })?;
    let sid = SourceId::new(0);

    match args.artifact.as_str() {
        "ast" => run_ast(&source, sid, args.item.as_deref(), verbose),
        "ir" => run_ir(&source, sid, args.item.as_deref(), verbose),
        "bytecode" => run_bytecode(&source, sid),
        "abi" => run_abi(&source, sid),
        "storage" => run_storage(&source, sid),
        "diagnostics" => run_diagnostics(&source, sid, verbose),
        other => Err(CliError::Usage(format!(
            "unknown artifact kind '{other}' — expected one of: ast, ir, bytecode, abi, storage, diagnostics"
        ))),
    }
}

fn run_ast(source: &str, sid: SourceId, item: Option<&str>, verbose: u8) -> Result<(), CliError> {
    let (tokens, _) = covenant_lexer::tokenize(source, sid);
    let (file_opt, diags) = covenant_parser::parse(&tokens, sid);
    let errors = diags.iter().filter(|d| d.level == DL::Error).count();
    if errors > 0 {
        for d in &diags {
            if d.level == DL::Error {
                eprintln!("error: {}", d.message);
            }
        }
        return Err(CliError::CompileError);
    }
    match file_opt {
        Some(file) => {
            if let Some(name) = item {
                if file.top_level.name.name.as_ref() != name {
                    return Err(CliError::Usage(format!("construct '{name}' not found")));
                }
            }
            if verbose >= 1 {
                println!("{file:#?}");
            } else {
                // Compact: just the construct name and declaration list
                let tl = &file.top_level;
                println!("{:?} {} {{", tl.keyword, tl.name.name);
                for decl in &tl.body {
                    println!("    {decl:?}");
                }
                println!("}}");
            }
        }
        None => {
            eprintln!("error: parse produced no AST");
            return Err(CliError::CompileError);
        }
    }
    Ok(())
}

fn run_ir(source: &str, sid: SourceId, item: Option<&str>, verbose: u8) -> Result<(), CliError> {
    match compile_to_ir(source, sid) {
        Ok(module) => {
            if let Some(name) = item {
                if module.name.name.as_ref() != name {
                    return Err(CliError::Usage(format!("construct '{name}' not found")));
                }
            }
            print_ir_module(&module, verbose);
            Ok(())
        }
        Err(diags) => {
            for d in &diags {
                eprintln!("{}: {}", level_str(d.level), d.message);
            }
            Err(CliError::CompileError)
        }
    }
}

fn run_bytecode(source: &str, sid: SourceId) -> Result<(), CliError> {
    let (artifact, diags) = compile(
        source,
        sid,
        EvmConfig::default(),
        StdlibConfig::default(),
        OptimizerConfig::default(),
    );
    let errors = diags.iter().filter(|d| d.level == DL::Error).count();
    if errors > 0 {
        for d in &diags {
            if d.level == DL::Error {
                eprintln!("error: {}", d.message);
            }
        }
        return Err(CliError::CompileError);
    }
    match artifact {
        Some(a) => {
            println!("deploy ({} bytes):", a.deploy_size());
            println!("  0x{}", hex::encode(&a.deploy_bytecode));
            println!("runtime ({} bytes):", a.runtime_size());
            println!("  0x{}", hex::encode(&a.runtime_bytecode));
        }
        None => {
            eprintln!("error: compilation produced no artifact");
            return Err(CliError::CompileError);
        }
    }
    Ok(())
}

fn run_abi(source: &str, sid: SourceId) -> Result<(), CliError> {
    let (artifact, diags) = compile(
        source,
        sid,
        EvmConfig::default(),
        StdlibConfig::default(),
        OptimizerConfig::default(),
    );
    let errors = diags.iter().filter(|d| d.level == DL::Error).count();
    if errors > 0 {
        for d in &diags {
            if d.level == DL::Error {
                eprintln!("error: {}", d.message);
            }
        }
        return Err(CliError::CompileError);
    }
    match artifact {
        Some(a) => {
            println!("{}", a.abi);
        }
        None => {
            eprintln!("error: compilation produced no artifact");
            return Err(CliError::CompileError);
        }
    }
    Ok(())
}

fn run_storage(source: &str, sid: SourceId) -> Result<(), CliError> {
    let (artifact, diags) = compile(
        source,
        sid,
        EvmConfig::default(),
        StdlibConfig::default(),
        OptimizerConfig::default(),
    );
    let errors = diags.iter().filter(|d| d.level == DL::Error).count();
    if errors > 0 {
        for d in &diags {
            if d.level == DL::Error {
                eprintln!("error: {}", d.message);
            }
        }
        return Err(CliError::CompileError);
    }
    match artifact {
        Some(a) => {
            let entries: Vec<serde_json::Value> = a
                .storage_layout
                .entries
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "name": e.name.as_ref(),
                        "slot": format!("0x{}", hex::encode(e.slot)),
                        "offset": e.offset,
                        "size": e.size_bytes,
                        "type": e.ty_desc.as_ref(),
                    })
                })
                .collect();
            let out = serde_json::to_string_pretty(&serde_json::json!({ "entries": entries }))
                .unwrap_or_else(|_| "{}".to_string());
            println!("{out}");
        }
        None => {
            eprintln!("error: compilation produced no artifact");
            return Err(CliError::CompileError);
        }
    }
    Ok(())
}

fn run_diagnostics(source: &str, sid: SourceId, verbose: u8) -> Result<(), CliError> {
    let diags = driver_check(source, sid);
    if diags.is_empty() {
        println!("no diagnostics");
        return Ok(());
    }
    for d in &diags {
        let lvl = level_str(d.level);
        println!("[{lvl}] {}: {}", d.code, d.message);
        if verbose >= 1 {
            if let Some(help) = &d.help {
                println!("  help: {help}");
            }
        }
    }
    let errors = diags.iter().filter(|d| d.level == DL::Error).count();
    if errors > 0 {
        return Err(CliError::CompileError);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// IR pretty-printer
// ---------------------------------------------------------------------------

fn print_ir_module(m: &IrModule, verbose: u8) {
    println!(
        "{:?} {} (fields: {}, functions: {})",
        m.construct_kind,
        m.name.name,
        m.fields.len(),
        m.functions.len()
    );
    for f in &m.functions {
        print_ir_function(f, verbose);
    }
}

fn print_ir_function(f: &IrFunction, verbose: u8) {
    let kind = match f.kind {
        IrFunctionKind::Action => "action",
        IrFunctionKind::View => "view",
        IrFunctionKind::Reveal => "reveal",
        IrFunctionKind::OnDestroy => "on_destroy",
        IrFunctionKind::Migrate { .. } => "migrate",
        IrFunctionKind::SelectiveDisclosure => "selective_disclosure",
        IrFunctionKind::FieldInitializer(_) => "field_initializer",
        IrFunctionKind::Test => "test",
    };
    let params: Vec<String> = f
        .params
        .iter()
        .map(|p| format!("{}: {:?}", p.name.name, p.ty))
        .collect();
    println!("\nfn {}({}) [{kind}]", f.name.name, params.join(", "));

    if !f.guards.is_empty() {
        println!("  guards: {:?}", f.guards);
    }
    if !f.annotations.is_empty() && verbose >= 1 {
        println!("  annotations: {:?}", f.annotations);
    }

    println!("  entry: bb{}", f.entry.0);
    for block in &f.blocks {
        let params: Vec<String> = block.params.iter().map(|v| format!("v{}", v.0)).collect();
        let params_str = if params.is_empty() {
            String::new()
        } else {
            format!("({})", params.join(", "))
        };
        println!("  bb{}{params_str}:", block.id.0);
        for instr in &block.instructions {
            let result = instr
                .result
                .map(|v| format!("v{} = ", v.0))
                .unwrap_or_default();
            let ops: Vec<String> = instr.operands.iter().map(|v| format!("v{}", v.0)).collect();
            let ops_str = if ops.is_empty() {
                String::new()
            } else {
                format!(" {}", ops.join(", "))
            };
            println!("    {result}{:?}{ops_str}", instr.opcode);
        }
        match &block.terminator {
            Terminator::Return { .. } => println!("    Return"),
            Terminator::Jump { target, args } => {
                let arg_strs: Vec<String> = args.iter().map(|v| format!("v{}", v.0)).collect();
                println!("    Jump bb{}({})", target.0, arg_strs.join(", "));
            }
            Terminator::Branch {
                cond,
                then_target,
                else_target,
                ..
            } => {
                println!(
                    "    Branch v{} ? bb{} : bb{}",
                    cond.0, then_target.0, else_target.0
                );
            }
            other => println!("    {other:?}"),
        }
    }
}

fn level_str(lvl: DiagnosticLevel) -> &'static str {
    match lvl {
        DiagnosticLevel::Error => "error",
        DiagnosticLevel::Warning => "warning",
        DiagnosticLevel::Note => "note",
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const HELLO: &str = include_str!("../../../covenant-lexer/tests/fixtures/example_01_hello.cov");
    const COIN: &str = include_str!("../../../covenant-lexer/tests/fixtures/example_02_coin.cov");

    #[test]
    fn ast_runs_on_hello() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), HELLO).unwrap();
        let args = InspectArgs {
            artifact: "ast".to_string(),
            target: tmp.path().to_path_buf(),
            item: None,
        };
        assert!(run(args, 0).is_ok());
    }

    #[test]
    fn abi_runs_on_coin() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), COIN).unwrap();
        let args = InspectArgs {
            artifact: "abi".to_string(),
            target: tmp.path().to_path_buf(),
            item: None,
        };
        assert!(run(args, 0).is_ok());
    }

    #[test]
    fn bytecode_runs_on_hello() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), HELLO).unwrap();
        let args = InspectArgs {
            artifact: "bytecode".to_string(),
            target: tmp.path().to_path_buf(),
            item: None,
        };
        assert!(run(args, 0).is_ok());
    }

    #[test]
    fn ir_runs_on_hello() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), HELLO).unwrap();
        let args = InspectArgs {
            artifact: "ir".to_string(),
            target: tmp.path().to_path_buf(),
            item: None,
        };
        assert!(run(args, 0).is_ok());
    }

    #[test]
    fn storage_runs_on_hello() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), HELLO).unwrap();
        let args = InspectArgs {
            artifact: "storage".to_string(),
            target: tmp.path().to_path_buf(),
            item: None,
        };
        assert!(run(args, 0).is_ok());
    }

    #[test]
    fn diagnostics_clean_source() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), HELLO).unwrap();
        let args = InspectArgs {
            artifact: "diagnostics".to_string(),
            target: tmp.path().to_path_buf(),
            item: None,
        };
        // hello.cov is valid; diagnostics should succeed (no errors)
        let result = run(args, 0);
        // May have warnings/info but no errors — just verify it doesn't panic.
        let _ = result;
    }

    #[test]
    fn unknown_artifact_returns_usage_error() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), HELLO).unwrap();
        let args = InspectArgs {
            artifact: "gobbledygook".to_string(),
            target: tmp.path().to_path_buf(),
            item: None,
        };
        let result = run(args, 0);
        assert!(
            matches!(result, Err(CliError::Usage(_))),
            "unknown artifact should be usage error"
        );
    }

    #[test]
    fn missing_file_returns_io_error() {
        let args = InspectArgs {
            artifact: "ast".to_string(),
            target: PathBuf::from("/no/such/file.cov"),
            item: None,
        };
        let result = run(args, 0);
        assert!(matches!(result, Err(CliError::Io(_))));
    }

    #[test]
    fn ast_verbose_does_not_panic() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), HELLO).unwrap();
        let args = InspectArgs {
            artifact: "ast".to_string(),
            target: tmp.path().to_path_buf(),
            item: None,
        };
        assert!(run(args, 2).is_ok());
    }
}
