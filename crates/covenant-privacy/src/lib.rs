//! Privacy flow analysis: enforces boundaries between encrypted and plaintext domains.
//!
//! Phase 5 of the compiler pipeline. Implements Doc 8 §3.1 (Property P1).
//! See also Doc 2 §4.4 and Doc 5 §9.

mod analyzer;
mod diag;
mod domain;
mod table;

use covenant_diag::{Diagnostic, SourceId};
use covenant_types::TypedFile;

pub use analyzer::Analyzer;
pub use domain::{domain_of, FlowContext, PrivacyDomain};
pub use table::PrivacyTable;

/// The full output of Phase 5: the typed AST plus per-expression privacy
/// domain annotations.
#[derive(Debug, Clone)]
pub struct PrivacyCheckedFile {
    pub typed: TypedFile,
    pub domains: PrivacyTable,
}

/// Run the privacy flow analyzer.
///
/// Always returns a `PrivacyCheckedFile` even with diagnostics. Every
/// expression span has a domain entry (possibly `Unknown`) so downstream
/// phases can trust the invariant "every span has a domain".
pub fn analyze_privacy(
    typed: TypedFile,
    _source_id: SourceId,
) -> (PrivacyCheckedFile, Vec<Diagnostic>) {
    Analyzer::new(typed).run()
}

/// Re-exported diagnostic codes (E301-E310, W301-W310).
pub mod codes {
    pub use crate::diag::{
        E301_PLAINTEXT_FROM_ENCRYPTED, E302_VIEW_ENCRYPTED_BODY, E303_REVEAL_MISSING_TARGET,
        E304_PROOF_PAYLOAD_CONTEXT, E305_DESTROY_CONTEXT, E306_REVERT_ENCRYPTED_ARG,
        E307_EMIT_ENCRYPTED_ARG, E308_SD_NON_BOOL, E309_CEREMONY_NO_ONDESTROY,
        E310_BRIDGE_NO_KEY_SWITCH, W301_REVEAL_ON_PLAINTEXT, W302_REVERT_PARAMS_IN_ENC,
        W303_EMIT_IN_ENC, W304_TRANSFER_IN_ENC, W305_MANY_ENC_COMPARISONS,
        W306_ENC_WHEN_PLAINTEXT_COND, W307_REVEAL_ONLY_CALLER, W308_SD_NO_PUBLIC_OUTPUT,
        W309_CEREMONY_NO_DESTROY_KEY, W310_BRIDGE_ENCRYPTED_FIELD,
    };
}

#[cfg(test)]
mod tests {
    #[test]
    fn smoke() {}
}
