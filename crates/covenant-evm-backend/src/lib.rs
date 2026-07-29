//! EVM bytecode code generation: lowers Covenant IR to EVM-compatible bytecode.
//!
//! Phase 8 of the compiler pipeline. Consumes an `IrModule` and produces an
//! `EvmArtifact` containing deploy + runtime bytecode, ABI JSON, selector
//! map, storage layout, and optional source map.

pub mod abi;
pub mod artifact;
pub mod asm;
pub mod codegen;
pub mod config;
pub mod diag;
pub mod mocked_crypto;
pub mod storage;
pub mod target;

use std::collections::BTreeMap;

use covenant_diag::{Diagnostic, Span};
use covenant_ir::IrModule;

pub use artifact::{
    CompilationMetadata, EvmArtifact, SourceMap, SourceMapEntry, StorageEntry, StorageLayout,
    PRECOMPILE_ABI_VERSION,
};
pub use config::{
    AllocatorStrategy, AmnesiaPrecompiles, EvmConfig, EvmVersion, FhePrecompiles, PqPrecompiles,
    PrecompileAddresses, ZkPrecompiles,
};
pub use target::{
    helper_selector_for_opcode, lift_v08_addr, EvmAddress, Target, TargetParseError,
    CEREMONY_HELPER_V090, FHE_HELPER_V090, PQ_HELPER_V090, ZK_HELPER_V090,
};

pub mod codes {
    pub use crate::diag::{
        E501_STACK_DEPTH, E502_UNKNOWN_OPCODE, E503_PRECOMPILE_UNSET, E504_STORAGE_OVERFLOW,
        E505_ABI_TYPE, E506_SELECTOR_COLLISION, E507_RUNTIME_TOO_LARGE, E508_DEPLOY_TOO_LARGE,
        E509_STRUCT_LAYOUT, E510_FHE_BRANCH_SIDE_EFFECTS, E511_ASSERT_ENCRYPTED,
        E512_LOG_TOO_MANY_TOPICS, E513_DYNAMIC_RETURN, E514_CIPHERTEXT_HANDLE,
        E515_UNRESOLVED_LABEL, E516_UNLOWERED_AMNESIA_OPCODE, E517_UNLOWERED_VDF_QUALIFIER,
        E518_UNLOWERED_BUILTIN_PREDICATE, E519_DIV_BY_ZERO_LITERAL, E520_HELPER_METHOD_MISSING,
        E521_TEXT_CONSTANT_TOO_LONG, E522_NESTED_MAP_UNSUPPORTED, E523_TRANSFER_FROM_UNSUPPORTED,
        E530_HEX_CONSTANT_TOO_LONG, E531_BARE_STRUCT_FIELD, E532_DYNAMIC_INDEXED_EVENT_PARAM,
        W501_LARGE_MEMORY, W502_LARGE_STORAGE, W503_SELECTOR_NEAR_COLLISION, W504_LARGE_RUNTIME,
        W505_EVENT_NO_INDEX, W506_MANY_PARAMS, W507_DYNAMIC_RETURN_NOT_ENCODED,
        W508_ONLY_CALLER_NOOP, W530_DYNAMIC_EVENT_DATA_NOT_ENCODED,
    };
}

/// Run EVM codegen.
pub fn codegen_evm(module: IrModule, config: EvmConfig) -> (EvmArtifact, Vec<Diagnostic>) {
    let mut cg = codegen::Codegen::new(&module, &config);
    let runtime_ops = cg.emit_runtime();
    let (runtime_bytecode, unresolved_runtime) = asm::assemble(&runtime_ops);
    for name in unresolved_runtime {
        cg.diagnostics.push(diag::unresolved_label(
            Span::new(module.source_id, 0, 0),
            &name,
        ));
    }

    let constructor_ops = cg.emit_constructor(runtime_bytecode.len() as u32);
    let (mut constructor_bytecode, unresolved_ctor) = asm::assemble(&constructor_ops);
    for name in unresolved_ctor {
        if name.as_ref() == "__runtime_start__" {
            // Expected: the runtime starts right after the constructor
            // bytecode. Compute the offset now.
            continue;
        }
        cg.diagnostics.push(diag::unresolved_label(
            Span::new(module.source_id, 0, 0),
            &name,
        ));
    }

    // Patch the `__runtime_start__` label: its value is the length of the
    // constructor bytecode (the runtime starts immediately after).
    let runtime_start = constructor_bytecode.len() as u32;
    patch_runtime_start(&mut constructor_bytecode, runtime_start);

    // Check size limits.
    if runtime_bytecode.len() > config.max_runtime_size as usize {
        cg.diagnostics.push(diag::runtime_too_large(
            Span::new(module.source_id, 0, 0),
            runtime_bytecode.len(),
            config.max_runtime_size as usize,
        ));
    }
    if runtime_bytecode.len() > 20 * 1024 {
        cg.diagnostics.push(diag::warn_large_runtime(
            Span::new(module.source_id, 0, 0),
            runtime_bytecode.len(),
        ));
    }

    // Check selector collisions.
    let mut selector_map = BTreeMap::new();
    for (name, sel) in &cg.selectors {
        if let Some(existing) = selector_map.get(sel).cloned() {
            let existing_name: Box<str> = existing;
            cg.diagnostics.push(diag::selector_collision(
                Span::new(module.source_id, 0, 0),
                &existing_name,
                name,
            ));
        } else {
            selector_map.insert(*sel, name.clone());
        }
    }

    // Check @slot annotation conflicts: KSR-CVN-041.
    let slot_conflicts = storage::detect_slot_collisions(&module);
    for (_, prior, slot) in slot_conflicts {
        cg.diagnostics
            .push(covenant_ir::diag::slot_annotation_conflict(
                Span::new(module.source_id, 0, 0),
                slot,
                &prior,
            ));
    }

    // Assemble full deploy bytecode: constructor + runtime.
    let mut deploy_bytecode = constructor_bytecode;
    deploy_bytecode.extend_from_slice(&runtime_bytecode);

    // Build ERC version table.
    let mut erc_versions: BTreeMap<Box<str>, Box<str>> = BTreeMap::new();
    erc_versions.insert("ERC-8227".into(), "0.1".into());
    erc_versions.insert("ERC-8228".into(), "0.1".into());
    erc_versions.insert("ERC-8229".into(), "0.1".into());
    erc_versions.insert("ERC-8231".into(), "0.1".into());

    let mocked_crypto_primitives = mocked_crypto::detect_mocked_crypto_usage(&module)
        .into_iter()
        .map(|u| Box::from(u.category))
        .collect();

    let metadata = CompilationMetadata {
        // KSR-CVN-006: keep in sync with `workspace.package.version`.
        covenant_version: env!("CARGO_PKG_VERSION").into(),
        optimizer_config: "default".into(),
        evm_version: config.evm_version,
        erc_versions,
        precompile_abi_version: artifact::PRECOMPILE_ABI_VERSION,
        mocked_crypto_primitives,
    };

    let abi_json = abi::emit_abi(&module, config.include_test_actions);
    let storage_layout = storage::compute_layout(&module);
    let function_selectors = cg.selectors.clone();
    let diags = cg.diagnostics.clone();

    let artifact = EvmArtifact {
        deploy_bytecode,
        runtime_bytecode,
        abi: abi_json,
        function_selectors,
        storage_layout,
        metadata,
        source_map: None,
    };

    (artifact, diags)
}

/// Patch a raw byte stream so any `PUSH2 0x00 0x00` that was meant to hold the
/// `__runtime_start__` label is replaced with `PUSH2 <runtime_start>`.
///
/// We take the simplifying assumption that the constructor has exactly one
/// `PUSH2 0x00 0x00` produced by a `PushLabel("__runtime_start__")`. The
/// constructor emits this just before the CODECOPY. We locate it by scanning
/// for the specific 3-byte pattern `0x61 0x00 0x00` preceding CODECOPY (0x39).
fn patch_runtime_start(bytes: &mut [u8], runtime_start: u32) {
    // PUSH2 = 0x61; we want to find a PUSH2 followed by CODECOPY.
    let mut i = 0;
    while i + 3 < bytes.len() {
        if bytes[i] == 0x61 && bytes[i + 1] == 0x00 && bytes[i + 2] == 0x00 {
            // Check downstream for CODECOPY to disambiguate. In V0 there's
            // only one such push pattern in the constructor.
            bytes[i + 1] = (runtime_start >> 8) as u8;
            bytes[i + 2] = runtime_start as u8;
            return;
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn smoke() {}
}
