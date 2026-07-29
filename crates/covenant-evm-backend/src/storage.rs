//! Storage layout computation: each declared field gets an assigned slot.
//!
//! V0 conventions:
//! * Sequential fields start at slot 0 and increment by 1 slot.
//! * Maps and lists use their own slot as a metadata slot; the actual entries
//!   live at keccak-derived slots computed at runtime via SHA3.
//! * Ciphertext fields store a 32-byte handle (slot-identical to a uint256).
//! * Structs are inlined; each struct field uses a consecutive slot.

use covenant_ir::{GlobalId, IrModule, Opcode, StructTypeId};
use covenant_types::Ty;

use crate::artifact::{StorageEntry, StorageLayout};

/// Conventional slot for the captured deployer address, written in the
/// constructor. Placed at a very-high slot index to avoid colliding with
/// user-declared fields.
pub const DEPLOYER_SLOT: u32 = 0xFFFFFFFE;

/// Reentrancy lock slot used by `@non_reentrant` guards.
/// A value of 1 means "entered"; 0 means "available".
pub const REENTRANT_LOCK_SLOT: u32 = 0xFFFFFFFF;

pub fn compute_layout(module: &IrModule) -> StorageLayout {
    let mut entries = Vec::new();
    let mut current = 0u32;

    for field in &module.fields {
        // KSR-CVN-021: honor `@slot(N)` annotation when present. The lint and
        // typecheck layers are responsible for warning about reserved-range
        // conflicts (EIP-1967) and for requiring the annotation on
        // `@proxy_compatible` contracts.
        let slot_num = field.explicit_slot.unwrap_or(current);
        let slot = slot_bytes(slot_num);
        // Render the RESOLVED type. Slot assignment below deliberately stays
        // on the declared one so this cannot move any existing contract's
        // fields: resolution is a description fix, not a layout change.
        let (size, desc) = describe(module, &resolved_field_ty(module, field.id, &field.ty));
        entries.push(StorageEntry {
            name: field.name.name.clone(),
            slot,
            offset: 0,
            size_bytes: size,
            ty_desc: desc,
        });
        // Only advance the sequential cursor for fields without a pinned slot,
        // so an explicit slot never silently displaces the next sequential
        // field.
        if field.explicit_slot.is_none() {
            current += slot_advance(&field.ty);
        }
    }

    StorageLayout { entries }
}

/// The resolved type of a declared field.
///
/// `IrField::ty` carries no resolution for nominal element types: a
/// `field rows: [Row]` arrives as `List(Unknown)` and a `field cfg: Cfg` as
/// plain `Unknown`, because struct names are bound to `StructId`s only while
/// typing expressions. The per-function SSA types DO carry it, so recover the
/// real type from the first `SLoad` of this field anywhere in the module.
///
/// Without this the storage sidecar rendered every `list<Struct>` as the
/// literal string `[_]`, so `covenant layout diff` (which compares name, slot
/// and type string) saw two byte-identical sidecars for a struct whose field
/// count had changed, and blessed an upgrade that relocated every element
/// after index 0.
///
/// Returns `declared` unchanged when nothing in the module loads the field, or
/// when the declared type was already resolved.
pub fn resolved_field_ty(module: &IrModule, id: GlobalId, declared: &Ty) -> Ty {
    if !contains_unknown(declared) {
        return declared.clone();
    }
    for f in &module.functions {
        for b in &f.blocks {
            for instr in &b.instructions {
                if instr.opcode != Opcode::SLoad(id) {
                    continue;
                }
                if let Some(ty) = instr.result.and_then(|v| f.value_types.get(&v)) {
                    if !contains_unknown(ty) {
                        return ty.clone();
                    }
                }
            }
        }
    }
    declared.clone()
}

fn contains_unknown(ty: &Ty) -> bool {
    match ty {
        Ty::Unknown => true,
        Ty::List(inner) | Ty::Ciphertext(inner) => contains_unknown(inner),
        Ty::Map(k, v) => contains_unknown(k) || contains_unknown(v),
        _ => false,
    }
}

fn slot_advance(ty: &Ty) -> u32 {
    match ty {
        Ty::List(_) => 1,   // length slot; data at keccak(slot) + i
        Ty::Map(_, _) => 1, // metadata slot; data at keccak(key || slot)
        Ty::Struct(_) => 4, // conservative placeholder; deep struct-size computation deferred
        _ => 1,
    }
}

fn describe(module: &IrModule, ty: &Ty) -> (u32, Box<str>) {
    match ty {
        Ty::Amount | Ty::Time | Ty::Duration => (32, "uint256".into()),
        Ty::Address => (20, "address".into()),
        Ty::Bool => (1, "bool".into()),
        Ty::Bytes => (32, "bytes".into()),
        Ty::Text => (32, "string".into()),
        Ty::Hash => (32, "bytes32".into()),
        Ty::PqKey => (32, "pq_key".into()),
        Ty::Ciphertext(_) => (32, "ciphertext<_>".into()),
        // The element stride belongs in the rendered type, not just the
        // element's name: `rows[i]` lives at `keccak(slot) + i * stride`, so
        // growing `struct Row { a b }` to `struct Row { a b c d }` relocates
        // every element after index 0. Rendering both versions as the bare
        // string `[_]` made the two sidecars byte-identical, and
        // `covenant layout diff` (which compares name, slot and type string)
        // blessed that upgrade as non-breaking. It is the only representation
        // of the stride the sidecar has.
        Ty::List(inner) => (
            32,
            format!(
                "[{}; stride={}]",
                render_ty(module, inner),
                stride_of(module, inner)
            )
            .into_boxed_str(),
        ),
        Ty::Map(k, v) => (
            32,
            format!("map<{}, {}>", render_ty(module, k), render_ty(module, v)).into_boxed_str(),
        ),
        Ty::Choice(_) => (1, "choice".into()),
        _ => (32, "unknown".into()),
    }
}

/// Consecutive storage words one element of a `[T]` occupies. Mirrors
/// `Codegen::list_elem_stride`, which is what the emitted address math
/// actually multiplies by.
fn stride_of(module: &IrModule, elem: &Ty) -> u32 {
    match elem {
        Ty::Struct(sid) => struct_field_count(module, sid.0).unwrap_or(1),
        _ => 1,
    }
}

fn struct_field_count(module: &IrModule, sid: u32) -> Option<u32> {
    module
        .structs
        .iter()
        .find(|s| s.id == StructTypeId(sid))
        .map(|s| (s.fields.len() as u32).max(1))
}

fn render_ty(module: &IrModule, t: &Ty) -> String {
    match t {
        Ty::Amount => "uint256".into(),
        Ty::Address => "address".into(),
        Ty::Bool => "bool".into(),
        Ty::Bytes => "bytes".into(),
        Ty::Text => "string".into(),
        Ty::Hash => "bytes32".into(),
        Ty::PqKey => "pq_key".into(),
        Ty::Ciphertext(inner) => format!("ciphertext<{}>", render_ty(module, inner)),
        Ty::List(inner) => format!(
            "[{}; stride={}]",
            render_ty(module, inner),
            stride_of(module, inner)
        ),
        Ty::Map(k, v) => format!("map<{}, {}>", render_ty(module, k), render_ty(module, v)),
        // Name the struct and its width. `"struct"` alone carried no field
        // count, so the sidecar had no representation of the layout at all.
        Ty::Struct(sid) => match module.structs.iter().find(|s| s.id == StructTypeId(sid.0)) {
            Some(s) => format!("struct {}({} words)", s.name.name, s.fields.len().max(1)),
            None => "struct".into(),
        },
        Ty::Unit => "()".into(),
        _ => "_".into(),
    }
}

pub fn slot_bytes(slot: u32) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    bytes[28..32].copy_from_slice(&slot.to_be_bytes());
    bytes
}

/// Map a field name → its assigned slot number. Used by codegen to know which
/// slot to SLOAD/SSTORE for a given `GlobalId`.
///
/// KSR-CVN-021: honors `@slot(N)` annotations — explicit slots are returned
/// as-is, and do not consume a position in the sequential counter.
pub fn slot_for_global(module: &IrModule, id: covenant_ir::GlobalId) -> u32 {
    let mut current = 0u32;
    for field in &module.fields {
        if field.id == id {
            return field.explicit_slot.unwrap_or(current);
        }
        if field.explicit_slot.is_none() {
            current += slot_advance(&field.ty);
        }
    }
    current
}

/// Detect duplicate slot assignments (sequential or explicit) in a module.
/// Returns a Vec of (conflicting_field_name, other_field_name, slot) tuples.
/// Used by codegen to emit E423 diagnostics (KSR-CVN-021).
pub fn detect_slot_collisions(module: &IrModule) -> Vec<(Box<str>, Box<str>, u32)> {
    let mut occupied: std::collections::BTreeMap<u32, Box<str>> = std::collections::BTreeMap::new();
    let mut collisions = Vec::new();
    let mut current = 0u32;
    for field in &module.fields {
        let slot = field.explicit_slot.unwrap_or(current);
        // KSR-CVN-042 (V0.9.2): an explicit `@slot(N)` that lands on a reserved
        // slot silently clobbers compiler-managed state — DEPLOYER_SLOT backs
        // `only deployer` auth and REENTRANT_LOCK_SLOT backs `@non_reentrant`.
        // Flag it through the same E423 path so the contract can't be built
        // with a hijacked deployer-auth or disabled reentrancy guard.
        if field.explicit_slot == Some(DEPLOYER_SLOT) {
            collisions.push((
                field.name.name.clone(),
                "<reserved: deployer-auth slot>".into(),
                slot,
            ));
        } else if field.explicit_slot == Some(REENTRANT_LOCK_SLOT) {
            collisions.push((
                field.name.name.clone(),
                "<reserved: reentrancy-lock slot>".into(),
                slot,
            ));
        }
        if let Some(prev) = occupied.get(&slot) {
            collisions.push((field.name.name.clone(), prev.clone(), slot));
        } else {
            occupied.insert(slot, field.name.name.clone());
        }
        if field.explicit_slot.is_none() {
            current += slot_advance(&field.ty);
        }
    }
    collisions
}
