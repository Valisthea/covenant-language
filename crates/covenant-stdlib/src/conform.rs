//! Shape checks shared by the standard-interface synthesizers.
//!
//! Every synthesizer in this crate reuses whatever the author already declared
//! when the NAME matches: a field named `balances`, an event named `Transfer`,
//! an error named `InsufficientBalance`. Reuse by name alone is what the V0.9.6
//! audit found: the synthesized bodies keep addressing the reused declaration
//! with the shape they were written against, so a mismatch ships an artifact
//! pair (ABI plus runtime, or sidecar plus runtime) that contradicts itself,
//! with no diagnostic anywhere. These helpers make the shape part of the match
//! and refuse when it does not hold, rather than reinterpreting the author's
//! declaration silently.

use covenant_diag::{Diagnostic, Span};
use covenant_ir::{
    id::GlobalId,
    instr::IrConstant,
    module::{IrField, IrMetadataValue},
    IrModule,
};
use covenant_parser::ast::{Ident, Principal, PrincipalKind};
use covenant_privacy::domain_of;
use covenant_types::Ty;

use crate::diag as d;

/// Render a type the way a `.cov` author writes it. `Ty::render` needs a
/// `TypeTable` this crate never sees, and the synthesizers only ever require
/// primitives, maps and ciphertexts, so a local renderer keeps the diagnostics
/// readable without threading the table through Phase 9.
pub fn ty_name(ty: &Ty) -> String {
    match ty {
        Ty::Amount => "amount".into(),
        Ty::Time => "time".into(),
        Ty::Duration => "duration".into(),
        Ty::Hash => "hash".into(),
        Ty::Text => "text".into(),
        Ty::Address => "address".into(),
        Ty::Bool => "bool".into(),
        Ty::Bytes => "bytes".into(),
        Ty::PqKey => "pq_key".into(),
        Ty::Ciphertext(inner) => format!("ciphertext<{}>", ty_name(inner)),
        Ty::List(inner) => format!("[{}]", ty_name(inner)),
        Ty::Map(k, v) => format!("map<{}, {}>", ty_name(k), ty_name(v)),
        other => format!("{other:?}"),
    }
}

/// Find an existing field by name, or inject one with the given type.
///
/// Returns `None` when a field of that name exists with a different type: the
/// synthesized surface has exactly one shape it can address per field, so
/// reusing a differently-typed one is a mis-compile, not an override (F-14).
/// The caller aborts synthesis on `None`; the pushed E609 already fails the
/// build, and stopping keeps a half-synthesized surface out of the artifact.
pub fn ensure_field(
    module: &mut IrModule,
    name: &str,
    ty: Ty,
    standard: &str,
    diags: &mut Vec<Diagnostic>,
) -> Option<GlobalId> {
    if let Some(existing) = module.fields.iter().find(|f| f.name.name.as_ref() == name) {
        if existing.ty != ty {
            diags.push(d::field_type_conflict(
                existing.span,
                name,
                &ty_name(&ty),
                &ty_name(&existing.ty),
                standard,
            ));
            return None;
        }
        return Some(existing.id);
    }
    let id = GlobalId(module.fields.len() as u32);
    let span = module.name.span;
    // `domain_of` so a ciphertext-typed field (ERC-8227 balances) lands in the
    // Encrypted domain rather than being force-labelled plaintext.
    let privacy = domain_of(&ty);
    module.fields.push(IrField {
        id,
        name: Ident {
            name: name.into(),
            span,
        },
        ty,
        privacy,
        initializer_fn: None,
        initializer_const: None,
        span,
        explicit_slot: None,
    });
    Some(id)
}

/// Name a `Principal` the way it is written in source, for diagnostics.
fn principal_name(p: &Principal) -> String {
    match p {
        Principal::Named(PrincipalKind::Deployer) => "deployer".into(),
        Principal::Named(PrincipalKind::Owner) => "owner".into(),
        Principal::Named(PrincipalKind::Admin) => "admin".into(),
        Principal::Named(PrincipalKind::Caller) => "caller".into(),
        Principal::Named(PrincipalKind::Guardians) => "guardians".into(),
        Principal::Named(PrincipalKind::Parties) => "parties".into(),
        Principal::Named(PrincipalKind::Holders) => "holders".into(),
        Principal::Predicate(id) => id.name.to_string(),
        Principal::Address(_) => "<a literal address>".into(),
        Principal::Call { name, .. } => format!("{}(..)", name.name),
    }
}

/// Apply the `supply: N to <principal>` genesis declaration to `total_supply`.
///
/// Returns `false` when the declaration cannot be honoured, in which case an
/// error is already pushed and the caller must abort synthesis. Two refusals
/// live here:
///
///   * a principal other than the literal `deployer` (F-20): the backend's
///     genesis path returns without minting, so the token deploys empty;
///   * a `total_supply` field default that disagrees with the genesis amount
///     (F-28): the default wins for the slot while `balances[deployer]` is
///     still seeded from the metadata, breaking `totalSupply() == sum(balances)`
///     at block zero.
pub fn apply_genesis_supply(
    module: &mut IrModule,
    total_supply_id: GlobalId,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    let span = module.name.span;
    let Some(IrMetadataValue::GenesisMint { amount, to }) = module.metadata.get("supply").cloned()
    else {
        return true;
    };

    if !matches!(to, Principal::Named(PrincipalKind::Deployer)) {
        diags.push(d::genesis_principal_unsupported(span, &principal_name(&to)));
        return false;
    }

    let Some(tsf) = module.fields.iter_mut().find(|f| f.id == total_supply_id) else {
        return true;
    };
    match &tsf.initializer_const {
        None => {
            tsf.initializer_const = Some(IrConstant::Integer(amount));
            true
        }
        // Same number written twice is not a contradiction, so it stays legal.
        Some(IrConstant::Integer(existing)) if *existing == amount => true,
        Some(IrConstant::Integer(existing)) => {
            let existing = Some(*existing);
            diags.push(d::supply_initializer_conflict(tsf.span, amount, existing));
            false
        }
        // A non-integer initializer on `total_supply` cannot be compared against
        // the genesis amount, so refuse rather than guess which one wins.
        Some(_) => {
            diags.push(d::supply_initializer_conflict(tsf.span, amount, None));
            false
        }
    }
}

/// Canonical parameter shape of a synthesized event or error: the type of each
/// parameter plus whether it is indexed. Parameter NAMES are deliberately not
/// part of the shape, because `from` / `to` are reserved keywords in Covenant
/// so a hand-written canonical `Transfer` has to name them something else.
pub type ParamShape<'a> = &'a [(Ty, bool)];

fn render_event_shape(name: &str, shape: ParamShape) -> String {
    let params: Vec<String> = shape
        .iter()
        .enumerate()
        .map(|(i, (ty, indexed))| {
            let idx = if *indexed { " indexed" } else { "" };
            format!("p{i}: {}{idx}", ty_name(ty))
        })
        .collect();
    format!("event {name}({})", params.join(", "))
}

fn render_error_shape(name: &str, shape: &[Ty]) -> String {
    let params: Vec<String> = shape
        .iter()
        .enumerate()
        .map(|(i, ty)| format!("p{i}: {}", ty_name(ty)))
        .collect();
    format!("error {name}({})", params.join(", "))
}

/// True when the module already declares an event of this name whose shape does
/// NOT match the canonical one; pushes E643 in that case (F-35).
pub fn event_shadow_conflicts(
    module: &IrModule,
    name: &str,
    canonical: ParamShape,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    let Some(existing) = module.events.iter().find(|e| e.name.name.as_ref() == name) else {
        return false;
    };
    let found: Vec<(Ty, bool)> = existing
        .params
        .iter()
        .map(|(_, ty, indexed)| (ty.clone(), *indexed))
        .collect();
    if found.as_slice() == canonical {
        return false;
    }
    diags.push(d::shadowed_synth_shape(
        existing.span,
        "event",
        name,
        &render_event_shape(name, canonical),
        &render_event_shape(name, &found),
    ));
    true
}

/// True when the module already declares an error of this name whose parameter
/// types do NOT match the canonical ones; pushes E643 in that case (F-35).
pub fn error_shadow_conflicts(
    module: &IrModule,
    name: &str,
    canonical: &[Ty],
    diags: &mut Vec<Diagnostic>,
) -> bool {
    let Some(existing) = module.errors.iter().find(|e| e.name.name.as_ref() == name) else {
        return false;
    };
    if existing.params.as_slice() == canonical {
        return false;
    }
    diags.push(d::shadowed_synth_shape(
        existing.span,
        "error",
        name,
        &render_error_shape(name, canonical),
        &render_error_shape(name, &existing.params),
    ));
    true
}

/// Refuse a `decimals:` value that cannot round-trip through EIP-20's `uint8`
/// return type (F-41). Returns `false` when the build must stop.
pub fn check_decimals(span: Span, value: u128, diags: &mut Vec<Diagnostic>) -> bool {
    if value > 255 {
        diags.push(d::decimals_unrepresentable(span, value));
        return false;
    }
    if value > 30 {
        diags.push(d::warn_decimals_range(span, value));
    }
    true
}
