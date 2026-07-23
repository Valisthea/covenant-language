use crate::{diagnostics::AsterDiagnostic, lowering::AsterIr};

pub fn validate(
    _aster_ir: &AsterIr<'_>,
    _warnings: &mut Vec<AsterDiagnostic>,
) -> Result<(), String> {
    // V0.7: trust EVM backend type checks.
    // V0.8: map each Covenant type to Aster VM native type:
    //   amount  → u256 (same)
    //   address → Aster account (different prefix)
    //   encrypted<T> → Aster FHE ciphertext
    //   pq_key  → Aster Dilithium public key
    //   ciphertext → Aster FHE blob
    Ok(())
}
