//! Standard library lowering: compiles Covenant built-ins into IR primitives.
//!
//! Phase 9 of the compiler pipeline. Consumes the `IrModule` emitted by Phase 6
//! and synthesizes standard-interface functions (ERC-20 for `token`, ERC-8227
//! for `confidential token`) by injecting `IrFunction`s, events, and errors
//! directly. For non-token constructs this phase is mostly a no-op in V0 —
//! ballot / ceremony / bridge / vault / registry synthesizers are stubbed and
//! emit `W606`.

mod amnesia_ceremony;
mod builder;
mod config;
mod diag;
mod erc20;
mod erc721;
mod erc8227;
mod erc8231;

use covenant_diag::{Diagnostic, SourceId};
use covenant_ir::IrModule;
use covenant_parser::ast::{ConstructKind, PrivacyQualifier};

pub use amnesia_ceremony::CANONICAL_SELECTORS as AMNESIA_CEREMONY_CANONICAL_SELECTORS;
pub use config::StdlibConfig;
pub use erc20::CANONICAL_SELECTORS;
pub use erc721::CANONICAL_SELECTORS as ERC721_CANONICAL_SELECTORS;
pub use erc8227::CANONICAL_SELECTORS as ERC8227_CANONICAL_SELECTORS;
pub use erc8231::CANONICAL_SELECTORS as ERC8231_CANONICAL_SELECTORS;

pub fn lower_stdlib(mut module: IrModule, config: StdlibConfig) -> (IrModule, Vec<Diagnostic>) {
    let mut diags = Vec::new();
    let _ = SourceId::new(0);

    match module.construct_kind {
        ConstructKind::Token => {
            if matches!(
                module.construct_privacy,
                Some(PrivacyQualifier::Confidential)
            ) {
                if config.synthesize_erc8227 {
                    erc8227::synthesize(&mut module, &config, &mut diags);
                }
            } else if config.synthesize_erc20 {
                erc20::synthesize(&mut module, &config, &mut diags);
            }
        }
        ConstructKind::Ballot => {
            if config.synthesize_ballot_queries {
                // Not implemented in V0.
            }
            diags.push(diag::warn_synth_not_impl(module.name.span, "ballot"));
        }
        ConstructKind::Ceremony if config.synthesize_ceremony_queries => {
            amnesia_ceremony::synthesize(&mut module, &config, &mut diags);
        }
        ConstructKind::Registry => {
            // V0.9 Sprint 35.b: ERC-8231 PQ key registry synthesis.
            if config.synthesize_erc8231 {
                erc8231::synthesize(&mut module, &config, &mut diags);
            } else {
                diags.push(diag::warn_synth_not_impl(module.name.span, "registry"));
            }
        }
        ConstructKind::Vault => {
            if config.synthesize_vault_queries {
                // Not implemented in V0.
            }
            diags.push(diag::warn_synth_not_impl(module.name.span, "vault"));
        }
        ConstructKind::Bridge => {
            diags.push(diag::warn_synth_not_impl(module.name.span, "bridge"));
        }
        ConstructKind::Nft => {
            // V0.9 Sprint 35.b: ERC-721 NFT synthesis.
            if config.synthesize_erc721 {
                erc721::synthesize(&mut module, &config, &mut diags);
            } else {
                diags.push(diag::warn_synth_not_impl(module.name.span, "nft"));
            }
        }
        // Record / Counter / Board / Market / Module / Test — no stdlib synthesis.
        _ => {}
    }

    (module, diags)
}

pub mod codes {
    pub use crate::diag::{
        E601_USER_FN_CONFLICT, E602_USER_FIELD_CONFLICT, E603_MISSING_REQUIRED_FIELD,
        E604_CONFIDENTIAL_MISSING, E605_SYNTHESIZED_INVALID, E606_BALLOT_NOT_IMPL,
        E607_BRIDGE_NOT_IMPL, E608_NESTED_MAP, E609_FIELD_TYPE_CONFLICT, E610_RESERVED_NAME,
        E611_CEREMONY_THRESHOLD_INVALID, W601_MISSING_METADATA, W602_USER_OVERRIDE,
        W603_NO_APPROVE, W604_EVENT_ALREADY_DECLARED, W605_ERC8227_UNEXERCISED,
        W606_SYNTH_NOT_IMPL, W607_AUTO_INJECTED_FIELD, W608_USER_ERROR_SHADOWS,
        W609_DECIMALS_RANGE, W610_EMPTY_SYMBOL,
    };
}

#[cfg(test)]
mod tests {
    #[test]
    fn smoke() {}
}
