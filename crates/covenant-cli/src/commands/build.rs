//! `covenant build`: compile .cov source to EVM artifacts.
//!
//! Supports two modes:
//!   - Single-file: `covenant build path/to/file.cov --out dir/`
//!   - Project:     `covenant build` (reads Covenant.toml)

use std::path::{Path, PathBuf};

use clap::Args;
use covenant_diag::{DiagnosticLevel, SourceId};
use covenant_driver::compile;
use covenant_evm_backend::{EvmConfig, Target};
use covenant_opt::OptimizerConfig;
use covenant_stdlib::StdlibConfig;

use crate::{diagnostics::emit_diagnostics, error::CliError, output::OutputFormat};

#[derive(Args, Debug, Clone)]
pub struct BuildArgs {
    /// Source file (.cov) for single-file mode, or omit for project mode.
    #[arg(value_name = "TARGET")]
    pub target: Option<PathBuf>,

    /// Output directory (overrides manifest).
    #[arg(long, short)]
    pub out: Option<PathBuf>,

    /// Disable optimizer.
    #[arg(long)]
    pub no_optimize: bool,

    /// Override backend target.
    ///
    /// EVM bytecode:
    ///
    ///   - `mockchain` / `mock`  → local mock chain, the default. Mocked
    ///     primitives run as native precompiles, nothing to deploy.
    ///   - `sepolia`             → all four helper contracts are deployed and
    ///     verified on Ethereum Sepolia. This is the testnet path that works.
    ///   - `aster_testnet`       → helper contracts NEVER verified deployed.
    ///     The addresses are the ones predicted for Sepolia, reused on the
    ///     assumption that the CREATE2 factory exists on that chain, which
    ///     nobody checked. A contract using any mocked primitive is refused
    ///     with E533 rather than shipped as a contract that reverts on first
    ///     use. Contracts that touch no mocked primitive build normally,
    ///     since their bytecode is the same on every target.
    ///
    /// Aster native bytecode:
    ///
    ///   - `aster`               → placeholder artifact only, not deployable.
    ///     Emits metadata and zero functions until the Aster SDK ships, and
    ///     warns loudly when you use it.
    ///
    /// There is no generic `evm` target. It previously aliased the local mock
    /// chain, whose helper addresses exist on no public network, so a build
    /// that read as portable produced bytecode that could not work anywhere.
    ///
    /// Mainnet is refused: this release is testnet-only, and mainnet is gated
    /// on an external audit.
    #[arg(long)]
    pub target_chain: Option<String>,

    /// Release mode: run security linter before build; block on Critical findings.
    #[arg(long)]
    pub release: bool,
}

pub fn run(
    args: &BuildArgs,
    manifest_path: Option<&PathBuf>,
    format: OutputFormat,
    use_color: bool,
) -> Result<(), CliError> {
    // Single-file mode: positional argument is a .cov path
    if let Some(ref t) = args.target {
        if t.extension().is_some_and(|e| e == "cov") && t.is_file() {
            let out = args.out.clone().unwrap_or_else(|| PathBuf::from("build"));
            return build_single_file(t, &out, args, format, use_color);
        }
    }

    // Project mode: read manifest
    let manifest_file = manifest_path
        .cloned()
        .or_else(|| covenant_manifest::Manifest::find_upward(&std::env::current_dir().unwrap()))
        .ok_or(CliError::ManifestNotFound)?;

    let manifest = covenant_manifest::Manifest::read(&manifest_file)?;

    let project_root = manifest_file.parent().unwrap().to_path_buf();
    let src_root = manifest
        .project
        .source
        .root
        .clone()
        .unwrap_or_else(|| PathBuf::from("src"));
    let entrypoints = manifest
        .project
        .source
        .entrypoints
        .clone()
        .unwrap_or_else(|| vec![PathBuf::from("main.cov")]);
    let out_dir = args
        .out
        .clone()
        .or_else(|| manifest.project.build.output.clone())
        .unwrap_or_else(|| PathBuf::from("build"));

    std::fs::create_dir_all(&out_dir)?;

    let mut any_error = false;
    for ep in &entrypoints {
        let full_path = project_root.join(&src_root).join(ep);
        if let Err(e) = build_single_file(&full_path, &out_dir, args, format, use_color) {
            match e {
                CliError::CompileError => any_error = true,
                other => return Err(other),
            }
        }
    }

    if any_error {
        return Err(CliError::CompileError);
    }
    Ok(())
}

pub fn build_single_file(
    source_path: &Path,
    out_dir: &Path,
    args: &BuildArgs,
    format: OutputFormat,
    use_color: bool,
) -> Result<(), CliError> {
    let source = std::fs::read_to_string(source_path).map_err(|e| {
        CliError::Io(std::io::Error::new(
            e.kind(),
            format!("cannot read `{}`: {e}", source_path.display()),
        ))
    })?;

    // Release mode: run linter before compilation and block on Critical findings.
    if args.release && super::lint::lint_for_release(source_path) {
        eprintln!(
            "error: linter found critical security findings in `{}`: \
             release build blocked (use `covenant lint` for details)",
            source_path.display()
        );
        return Err(CliError::LintError);
    }

    let source_id = SourceId::new(0);
    let path_str = source_path.to_string_lossy();

    // Aster native backend: bypass EVM bytecode entirely.
    // (Distinct from `aster_testnet` which targets EVM bytecode that calls
    // V0.9 helpers deployed on Aster's EVM-compatible layer.)
    if args.target_chain.as_deref() == Some("aster") {
        return build_aster_target(source_path, &source, source_id, out_dir, format);
    }

    // V0.9: resolve EVM precompile target. Default = MockChain (V0.8 behavior).
    let mut real_chain_target: Option<Target> = None;
    let evm_config = match args.target_chain.as_deref() {
        None => EvmConfig::default(),
        Some(s) => match Target::parse(s) {
            Ok(target) => {
                if target.uses_helper_contracts() {
                    real_chain_target = Some(target);
                }
                EvmConfig::for_target(target)
            }
            Err(e) => {
                eprintln!("error: {e}");
                return Err(CliError::CompileError);
            }
        },
    };

    let opt_config = if args.no_optimize {
        OptimizerConfig::none()
    } else {
        OptimizerConfig::default()
    };

    let (artifact_opt, diags) = compile(
        &source,
        source_id,
        evm_config,
        StdlibConfig::default(),
        opt_config,
    );

    let error_count = diags
        .iter()
        .filter(|d| d.level == DiagnosticLevel::Error)
        .count();
    emit_diagnostics(&diags, &path_str, &source, format, use_color);

    if error_count > 0 || artifact_opt.is_none() {
        if format == OutputFormat::Json {
            println!(
                "{}",
                serde_json::json!({
                    "file": path_str,
                    "status": "error",
                    "errors": error_count,
                })
            );
        } else {
            eprintln!("error: compilation failed ({error_count} error(s))");
        }
        return Err(CliError::CompileError);
    }

    let artifact = artifact_opt.unwrap();

    // OMEGA V6 (HGH-031 fix): mirror `build_aster_target`'s existing
    // precedent below -- warn loudly, at the layer a `covenant` CLI user
    // and any downstream integrator/auditor actually sees, when a real
    // public-testnet build routes an FHE/PQ/ZK primitive to a `Mocked*.sol`
    // helper contract instead of real cryptography.
    if let Some(target) = real_chain_target {
        for primitive in artifact.metadata.mocked_crypto_primitives.iter() {
            let helper =
                covenant_evm_backend::mocked_crypto::helper_contract_for_category(primitive);
            eprintln!(
                "warning[mocked-crypto]: this contract's `{primitive}` primitive calls out to \
                 `{helper}` on {} -- a V0.9 placeholder standing in for real cryptography \
                 (see helpers/src/{helper}.sol: \"NOT FOR PRODUCTION SECRETS\"), not the \
                 Dilithium-5/TFHE/Halo2 guarantees the source implies.",
                target.as_str()
            );
        }
    }

    // Derive construct name from source (first top-level declaration name).
    let construct_name = derive_construct_name(source_path, &source);
    let stem = sanitize(&construct_name);

    std::fs::create_dir_all(out_dir)?;
    write_file(
        out_dir,
        &format!("{stem}.bin"),
        hex::encode(&artifact.deploy_bytecode),
    )?;
    write_file(
        out_dir,
        &format!("{stem}.runtime.bin"),
        hex::encode(&artifact.runtime_bytecode),
    )?;
    write_file(out_dir, &format!("{stem}.abi.json"), artifact.abi.clone())?;
    write_file(
        out_dir,
        &format!("{stem}.storage.json"),
        serialize_storage_layout(&artifact.storage_layout),
    )?;
    write_file(
        out_dir,
        &format!("{stem}.metadata.json"),
        serialize_metadata(&artifact.metadata, &artifact.function_selectors),
    )?;

    match format {
        OutputFormat::Human => {
            println!(
                "ok: {stem}: deploy {} bytes, runtime {} bytes → {}",
                artifact.deploy_bytecode.len(),
                artifact.runtime_bytecode.len(),
                out_dir.display()
            );
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::json!({
                    "construct": stem,
                    "deploy_bytes": artifact.deploy_bytecode.len(),
                    "runtime_bytes": artifact.runtime_bytecode.len(),
                    "status": "ok",
                })
            );
        }
    }

    Ok(())
}

fn derive_construct_name(path: &Path, source: &str) -> String {
    let source_id = SourceId::new(0);
    let (tokens, _) = covenant_lexer::tokenize(source, source_id);
    let (file_opt, _) = covenant_parser::parse(&tokens, source_id);
    file_opt
        .map(|f| f.top_level.name.name.to_string())
        .unwrap_or_else(|| {
            path.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "contract".to_string())
        })
}

fn write_file(dir: &Path, name: &str, content: String) -> Result<(), CliError> {
    let p = dir.join(name);
    std::fs::write(&p, content).map_err(|e| {
        CliError::Io(std::io::Error::new(
            e.kind(),
            format!("cannot write `{}`: {e}", p.display()),
        ))
    })
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn serialize_storage_layout(layout: &covenant_evm_backend::StorageLayout) -> String {
    let entries: Vec<serde_json::Value> = layout
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
    serde_json::to_string_pretty(&serde_json::json!({ "entries": entries }))
        .unwrap_or_else(|_| "{}".to_string())
}

fn serialize_metadata(
    meta: &covenant_evm_backend::CompilationMetadata,
    selectors: &std::collections::BTreeMap<Box<str>, [u8; 4]>,
) -> String {
    let erc: serde_json::Map<String, serde_json::Value> = meta
        .erc_versions
        .iter()
        .map(|(k, v)| (k.to_string(), serde_json::Value::String(v.to_string())))
        .collect();
    let sels: serde_json::Map<String, serde_json::Value> = selectors
        .iter()
        .map(|(k, v)| {
            (
                k.to_string(),
                serde_json::Value::String(format!("0x{}", hex::encode(v))),
            )
        })
        .collect();
    serde_json::to_string_pretty(&serde_json::json!({
        "covenantVersion": meta.covenant_version.as_ref(),
        "optimizerConfig": meta.optimizer_config.as_ref(),
        "evmVersion": format!("{:?}", meta.evm_version),
        "erc": erc,
        // KSR-CVN-029: precompile-ABI version the bytecode targets.
        "precompileAbiVersion": meta.precompile_abi_version,
        "functionSelectors": sels,
        // OMEGA V6 (HGH-031 fix): FHE/PQ/ZK primitive categories this
        // artifact routes to a `Mocked*.sol` helper contract for, so a
        // downstream integrator/auditor reading only the metadata (not the
        // compiler internals) has the same signal a CLI user sees.
        "mockedCryptoPrimitives": meta.mocked_crypto_primitives.iter().map(|c| c.as_ref()).collect::<Vec<_>>(),
    }))
    .unwrap_or_else(|_| "{}".to_string())
}

fn build_aster_target(
    source_path: &Path,
    source: &str,
    source_id: SourceId,
    out_dir: &Path,
    format: OutputFormat,
) -> Result<(), CliError> {
    eprintln!(
        "warning[aster-sdk-pending]: Aster target is V0.7 foundation mode, \
         artifact is placeholder, not deployable. Full emission requires Aster SDK."
    );

    let ir = covenant_driver::compile_to_ir(source, source_id).map_err(|diags| {
        for d in &diags {
            eprintln!("error: {}", d.message);
        }
        CliError::CompileError
    })?;

    let artifact = covenant_aster_backend::compile_module(&ir).map_err(|errs| {
        for e in &errs {
            eprintln!("error[aster]: {e}");
        }
        CliError::CompileError
    })?;

    for w in &artifact.warnings {
        eprintln!("  [{}] {}", w.code, w.message);
    }

    let construct_name = derive_construct_name(source_path, source);
    let stem = sanitize(&construct_name);

    std::fs::create_dir_all(out_dir)?;
    let out_path = out_dir.join(format!("{stem}.aster"));
    std::fs::write(&out_path, &artifact.bytecode).map_err(|e| {
        CliError::Io(std::io::Error::new(
            e.kind(),
            format!("cannot write `{}`: {e}", out_path.display()),
        ))
    })?;

    let meta_path = out_dir.join(format!("{stem}.aster.json"));
    let meta_json = serde_json::to_string_pretty(&serde_json::json!({
        "metadata": artifact.metadata,
        "abi": artifact.abi,
    }))
    .unwrap_or_else(|_| "{}".to_string());
    std::fs::write(&meta_path, meta_json).map_err(|e| {
        CliError::Io(std::io::Error::new(
            e.kind(),
            format!("cannot write `{}`: {e}", meta_path.display()),
        ))
    })?;

    match format {
        OutputFormat::Human => {
            println!(
                "ok[aster]: {stem}: {} bytes → {} (chain 1996, sdk_lowering=false)",
                artifact.bytecode.len(),
                out_dir.display()
            );
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::json!({
                    "construct": stem,
                    "target": "aster",
                    "chain_id": 1996,
                    "bytes": artifact.bytecode.len(),
                    "sdk_lowering": false,
                    "status": "ok",
                })
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_plain() {
        assert_eq!(sanitize("Coin"), "Coin");
    }

    #[test]
    fn sanitize_spaces() {
        assert_eq!(sanitize("My Token"), "My_Token");
    }

    #[test]
    fn sanitize_hyphens_preserved() {
        assert_eq!(sanitize("my-contract"), "my-contract");
    }
}
