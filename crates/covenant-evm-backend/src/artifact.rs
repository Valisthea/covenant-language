//! The backend's output container.

use std::collections::BTreeMap;

use covenant_diag::{SourceId, Span};

use crate::config::EvmVersion;

#[derive(Debug, Clone)]
pub struct EvmArtifact {
    pub deploy_bytecode: Vec<u8>,
    pub runtime_bytecode: Vec<u8>,
    pub abi: String,
    pub function_selectors: BTreeMap<Box<str>, [u8; 4]>,
    pub storage_layout: StorageLayout,
    pub metadata: CompilationMetadata,
    pub source_map: Option<SourceMap>,
}

#[derive(Debug, Clone, Default)]
pub struct StorageLayout {
    pub entries: Vec<StorageEntry>,
}

#[derive(Debug, Clone)]
pub struct StorageEntry {
    pub name: Box<str>,
    pub slot: [u8; 32],
    pub offset: u8,
    pub size_bytes: u32,
    pub ty_desc: Box<str>,
}

#[derive(Debug, Clone)]
pub struct CompilationMetadata {
    pub covenant_version: Box<str>,
    pub optimizer_config: Box<str>,
    pub evm_version: EvmVersion,
    pub erc_versions: BTreeMap<Box<str>, Box<str>>,
    /// KSR-CVN-029: precompile-ABI version the bytecode was compiled against.
    /// A chain deploying a different precompile ABI MUST reject a contract
    /// whose `precompile_abi_version` does not match the chain's current
    /// value. This field is surfaced in `metadata.json` and in the on-chain
    /// constant block near the start of the runtime bytecode.
    pub precompile_abi_version: u32,
    /// OMEGA V6 (HGH-031 fix): FHE/PQ/ZK primitive categories ("fhe", "pq",
    /// "zk") this artifact's bytecode calls out to a `Mocked*.sol` helper
    /// contract for. Populated regardless of target (it's ground truth
    /// about what the bytecode does); the CLI only *warns* about it when
    /// the target is a real chain (Sepolia / AsterTestnet).
    pub mocked_crypto_primitives: Vec<Box<str>>,
}

// KSR-CVN-040: single source of truth lives in `crate::abi`; re-export here
// so existing callers (codegen.rs) don't need a path change.
pub use crate::abi::PRECOMPILE_ABI_VERSION;

#[derive(Debug, Clone, Default)]
pub struct SourceMap {
    pub entries: Vec<SourceMapEntry>,
}

#[derive(Debug, Clone)]
pub struct SourceMapEntry {
    pub offset: u32,
    pub source_id: SourceId,
    pub span: Span,
    pub instr_kind: Box<str>,
}

impl EvmArtifact {
    pub fn runtime_size(&self) -> usize {
        self.runtime_bytecode.len()
    }
    pub fn deploy_size(&self) -> usize {
        self.deploy_bytecode.len()
    }
}
