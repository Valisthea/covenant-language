//! IR → Aster IR lowering (V0.7 foundation).

use crate::diagnostics::AsterDiagnostic;
use covenant_ir::IrModule;

/// Lightweight Aster IR wrapper.
///
/// V0.7: holds reference to source IR + metadata needed by bytecode emitter.
/// V0.8: full Aster-native IR structure once SDK is available.
pub struct AsterIr<'a> {
    pub source: &'a IrModule,
    pub function_count: usize,
}

pub fn lower<'a>(
    module: &'a IrModule,
    warnings: &mut Vec<AsterDiagnostic>,
) -> Result<AsterIr<'a>, String> {
    warnings.push(AsterDiagnostic::warning(
        "aster-sdk-pending",
        "Aster backend V0.7 foundation: placeholder output only. \
         Full bytecode emission requires Aster SDK (pending GA). \
         Do not deploy this artifact.",
    ));

    Ok(AsterIr {
        function_count: module.functions.len(),
        source: module,
    })
}
