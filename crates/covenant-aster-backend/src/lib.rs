//! Aster Chain native code generation for Covenant IR.
//!
//! **V0.7.0: foundation with placeholder lowering.**
//! Full Aster bytecode emission requires Aster SDK (pending GA).
//!
//! This crate provides:
//! - [`compile_module`]: public compilation entry point
//! - [`AsterCompilationArtifact`]: output type
//! - Placeholder emission that produces clear SDK-pending diagnostics
//! - Structure ready for real lowering once the Aster SDK is available
//!
//! # Target chain
//!
//! Aster Chain (chainId 1996), PoSA Layer 1, 50 ms block time.
//! Non-EVM architecture: see <https://asterdex.com>.

pub mod bytecode;
pub mod diagnostics;
pub mod lowering;
pub mod precompiles;
pub mod types;

pub use bytecode::ASTER_MAGIC;
pub use diagnostics::AsterDiagnostic;

use serde::{Deserialize, Serialize};

/// Compilation output for an Aster Chain target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsterCompilationArtifact {
    /// Raw artifact bytes. V0.7: magic prefix + metadata only.
    pub bytecode: Vec<u8>,
    /// ABI descriptor (JSON). V0.7: empty stubs.
    pub abi: serde_json::Value,
    /// Compilation metadata.
    pub metadata: AsterMetadata,
    /// Non-fatal diagnostics emitted during compilation.
    pub warnings: Vec<AsterDiagnostic>,
}

/// Aster-specific compilation metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsterMetadata {
    /// Covenant compiler version.
    pub compiler_version: String,
    /// Aster IR format version (currently 1).
    pub ir_version: u32,
    /// Target chain ID (Aster: 1996).
    pub chain_id: u64,
    /// Target block time in ms (Aster: 50).
    pub target_block_time_ms: u32,
    /// Whether real SDK lowering was used (false in V0.7).
    pub sdk_lowering: bool,
}

impl AsterMetadata {
    pub fn new() -> Self {
        Self {
            compiler_version: env!("CARGO_PKG_VERSION").to_string(),
            ir_version: 1,
            chain_id: 1996,
            target_block_time_ms: 50,
            sdk_lowering: false,
        }
    }
}

impl Default for AsterMetadata {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors returned by Aster backend compilation.
#[derive(Debug, Clone, thiserror::Error)]
pub enum AsterError {
    #[error("Aster SDK integration pending: cannot emit production bytecode")]
    SdkPending,

    #[error("Lowering failed: {0}")]
    LoweringFailed(String),

    #[error("Type mapping failed: {0}")]
    TypeMappingFailed(String),

    #[error("Precompile resolution failed: {0}")]
    PrecompileResolutionFailed(String),

    #[error("Bytecode emission failed: {0}")]
    BytecodeEmissionFailed(String),
}

/// Compile a Covenant IR module to an Aster Chain artifact.
///
/// # V0.7 behaviour
///
/// Returns a placeholder artifact with SDK-pending warnings.
/// The pipeline structure mirrors the full lowering so V0.8 can slot in
/// real implementations at each phase without structural changes.
pub fn compile_module(
    module: &covenant_ir::IrModule,
) -> Result<AsterCompilationArtifact, Vec<AsterError>> {
    let mut warnings = Vec::new();

    let aster_ir =
        lowering::lower(module, &mut warnings).map_err(|e| vec![AsterError::LoweringFailed(e)])?;

    types::validate(&aster_ir, &mut warnings)
        .map_err(|e| vec![AsterError::TypeMappingFailed(e)])?;

    let precompile_map = precompiles::resolve(&aster_ir, &mut warnings)
        .map_err(|e| vec![AsterError::PrecompileResolutionFailed(e)])?;

    let bytecode = bytecode::emit(&aster_ir, &precompile_map, &mut warnings)
        .map_err(|e| vec![AsterError::BytecodeEmissionFailed(e)])?;

    let abi = serde_json::json!({
        "version": "aster-v0.7-foundation",
        "functions": [],
        "events": [],
        "errors": [],
    });

    Ok(AsterCompilationArtifact {
        bytecode,
        abi,
        metadata: AsterMetadata::new(),
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_targets_aster_chain() {
        let meta = AsterMetadata::new();
        assert_eq!(meta.chain_id, 1996);
        assert_eq!(meta.target_block_time_ms, 50);
        assert!(!meta.sdk_lowering);
        assert!(meta.compiler_version.contains('.'));
    }
}
