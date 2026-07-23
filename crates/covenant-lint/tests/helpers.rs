//! Shared helpers for covenant-lint integration tests.

use covenant_diag::SourceId;
use covenant_ir::IrModule;
use covenant_lint::framework::{Detector, Finding};

pub fn compile(src: &str) -> IrModule {
    covenant_driver::compile_to_ir(src, SourceId::new(0))
        .unwrap_or_else(|diags| panic!("compile failed: {diags:?}"))
}

#[allow(dead_code)]
pub fn try_compile(src: &str) -> Result<IrModule, Vec<covenant_diag::Diagnostic>> {
    covenant_driver::compile_to_ir(src, SourceId::new(0))
}

pub fn run(det: &dyn Detector, src: &str) -> Vec<Finding> {
    let ir = compile(src);
    det.analyze(&ir, src)
}

#[allow(dead_code)]
pub fn codes(findings: &[Finding]) -> Vec<&str> {
    findings.iter().map(|f| f.detector_code).collect()
}
