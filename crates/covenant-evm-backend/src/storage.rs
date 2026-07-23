//! Storage layout computation: each declared field gets an assigned slot.
//!
//! V0 conventions:
//! * Sequential fields start at slot 0 and increment by 1 slot.
//! * Maps and lists use their own slot as a metadata slot; the actual entries
//!   live at keccak-derived slots computed at runtime via SHA3.
//! * Ciphertext fields store a 32-byte handle (slot-identical to a uint256).
//! * Structs are inlined; each struct field uses a consecutive slot.

use covenant_ir::IrModule;
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
        let (size, desc) = describe(&field.ty);
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

fn slot_advance(ty: &Ty) -> u32 {
    match ty {
        Ty::List(_) => 1,   // length slot; data at keccak(slot) + i
        Ty::Map(_, _) => 1, // metadata slot; data at keccak(key || slot)
        Ty::Struct(_) => 4, // conservative placeholder; deep struct-size computation deferred
        _ => 1,
    }
}

fn describe(ty: &Ty) -> (u32, Box<str>) {
    match ty {
        Ty::Amount | Ty::Time | Ty::Duration => (32, "uint256".into()),
        Ty::Address => (20, "address".into()),
        Ty::Bool => (1, "bool".into()),
        Ty::Bytes => (32, "bytes".into()),
        Ty::Text => (32, "string".into()),
        Ty::Hash => (32, "bytes32".into()),
        Ty::PqKey => (32, "pq_key".into()),
        Ty::Ciphertext(_) => (32, "ciphertext<_>".into()),
        Ty::List(inner) => (32, format!("[{}]", render_ty(inner)).into_boxed_str()),
        Ty::Map(k, v) => (
            32,
            format!("map<{}, {}>", render_ty(k), render_ty(v)).into_boxed_str(),
        ),
        Ty::Choice(_) => (1, "choice".into()),
        _ => (32, "unknown".into()),
    }
}

fn render_ty(t: &Ty) -> String {
    match t {
        Ty::Amount => "uint256".into(),
        Ty::Address => "address".into(),
        Ty::Bool => "bool".into(),
        Ty::Bytes => "bytes".into(),
        Ty::Text => "string".into(),
        Ty::Hash => "bytes32".into(),
        Ty::PqKey => "pq_key".into(),
        Ty::Ciphertext(inner) => format!("ciphertext<{}>", render_ty(inner)),
        Ty::List(inner) => format!("[{}]", render_ty(inner)),
        Ty::Map(k, v) => format!("map<{}, {}>", render_ty(k), render_ty(v)),
        Ty::Struct(_) => "struct".into(),
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
