//! Aster bytecode emission (V0.7 placeholder).

use crate::{diagnostics::AsterDiagnostic, lowering::AsterIr, precompiles::PrecompileMap};

/// Magic prefix for Aster-targeted Covenant artifacts: "COV7" + IR format version 1.
pub const ASTER_MAGIC: &[u8] = b"COV7\x01";

pub fn emit(
    aster_ir: &AsterIr<'_>,
    _precompile_map: &PrecompileMap,
    warnings: &mut Vec<AsterDiagnostic>,
) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();

    // Magic identifier
    out.extend_from_slice(ASTER_MAGIC);

    // Function count (4 bytes LE) — minimal metadata
    let fc = aster_ir.function_count as u32;
    out.extend_from_slice(&fc.to_le_bytes());

    warnings.push(AsterDiagnostic::info(
        "aster-placeholder-bytecode",
        &format!(
            "Emitted {} bytes placeholder Aster artifact (magic + metadata, {} functions). \
             Real bytecode emission requires Aster SDK.",
            out.len(),
            aster_ir.function_count,
        ),
    ));

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magic_is_cov7() {
        assert_eq!(&ASTER_MAGIC[..4], b"COV7");
        assert_eq!(ASTER_MAGIC[4], 1);
    }
}
