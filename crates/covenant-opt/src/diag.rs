//! Optimizer diagnostic codes (E401-E410, W401-W410).
#![allow(dead_code)]

use covenant_diag::{DiagCode, Diagnostic, DiagnosticLevel, Span};

pub const E401_FIXPOINT: DiagCode = DiagCode(401);
pub const E403_INVARIANT_BROKEN: DiagCode = DiagCode(403);
pub const E404_TYPE_INCONSISTENCY: DiagCode = DiagCode(404);
pub const E405_CSE_TYPE_MISMATCH: DiagCode = DiagCode(405);

pub const W401_UNUSED_FIELD: DiagCode = DiagCode(401);
pub const W402_UNUSED_LOCAL: DiagCode = DiagCode(402);
pub const W403_UNREACHABLE: DiagCode = DiagCode(403);
pub const W404_UNUSED_EVENT: DiagCode = DiagCode(404);
pub const W405_PRECOMPUTE_RUNTIME: DiagCode = DiagCode(405);
pub const W406_BATCH_NOT_ACTIONABLE: DiagCode = DiagCode(406);
pub const W407_LARGE_SSA: DiagCode = DiagCode(407);
pub const W408_MANY_SLOAD_COALESCED: DiagCode = DiagCode(408);
pub const W409_MANY_CSE: DiagCode = DiagCode(409);
pub const W410_LARGE_FOLDING: DiagCode = DiagCode(410);

fn warn(code: DiagCode, msg: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic {
        level: DiagnosticLevel::Warning,
        code,
        message: msg.into(),
        span,
        help: None,
    }
}

pub fn fixpoint_not_converged(span: Span, iters: u32) -> Diagnostic {
    Diagnostic::error(
        E401_FIXPOINT,
        format!("optimizer fixpoint did not converge within {iters} iterations"),
        span,
    )
}

pub fn invariant_broken(span: Span, pass: &str, detail: &str) -> Diagnostic {
    Diagnostic::error(
        E403_INVARIANT_BROKEN,
        format!("pass `{pass}` broke IR invariant: {detail}"),
        span,
    )
}

pub fn warn_unused_field(span: Span, name: &str) -> Diagnostic {
    warn(
        W401_UNUSED_FIELD,
        format!("field `{name}` is declared but never accessed"),
        span,
    )
}

pub fn warn_unused_local(span: Span) -> Diagnostic {
    warn(W402_UNUSED_LOCAL, "local binding is never read", span)
}

pub fn warn_unreachable(span: Span) -> Diagnostic {
    warn(W403_UNREACHABLE, "block is unreachable", span)
}

pub fn warn_unused_event(span: Span, name: &str) -> Diagnostic {
    warn(
        W404_UNUSED_EVENT,
        format!("event `{name}` is declared but never emitted"),
        span,
    )
}

pub fn warn_precompute_runtime(span: Span) -> Diagnostic {
    warn(
        W405_PRECOMPUTE_RUNTIME,
        "@precompute expression depends on runtime state; fallback to per-call evaluation",
        span,
    )
}

pub fn warn_many_sload_coalesced(span: Span, n: usize) -> Diagnostic {
    warn(
        W408_MANY_SLOAD_COALESCED,
        format!("{n} redundant SLoads coalesced; consider caching the field in a local"),
        span,
    )
}

pub fn warn_many_cse(span: Span, n: usize) -> Diagnostic {
    warn(
        W409_MANY_CSE,
        format!("{n} redundant computations eliminated by CSE"),
        span,
    )
}
