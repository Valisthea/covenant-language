//! ABI type mapping, signature computation, selector computation, and JSON
//! emission.

use covenant_ir::{
    instr::Terminator, module::IrEvent, IrFunction, IrFunctionKind, IrModule, Opcode,
};
use covenant_types::Ty;
use sha3::{Digest, Keccak256};

/// Map a Covenant type to its Solidity ABI-JSON type name.
pub fn abi_type_of(t: &Ty) -> String {
    match t {
        Ty::Amount | Ty::Time | Ty::Duration => "uint256".to_string(),
        Ty::Address => "address".to_string(),
        Ty::Bool => "bool".to_string(),
        Ty::Bytes => "bytes".to_string(),
        Ty::Text => "string".to_string(),
        Ty::Hash => "bytes32".to_string(),
        Ty::PqKey => "bytes".to_string(),
        Ty::Ciphertext(_) => "bytes32".to_string(), // ciphertext handle
        Ty::List(inner) => format!("{}[]", abi_type_of(inner)),
        Ty::Map(_, _) => "bytes".to_string(), // maps aren't first-class in calldata; placeholder
        Ty::Struct(_) | Ty::Credential(_) => "bytes".to_string(), // simplified for V0
        Ty::Choice(_) => "uint8".to_string(),
        Ty::Unit => "".to_string(),
        _ => "bytes".to_string(),
    }
}

/// The type actually returned by a function whose `returns` clause is absent
/// (the `reveal <field> to <target>` one-liner): trace the first
/// `Return(Some(v))` terminator to the returned value's type. This is the
/// decrypted field type for an encrypted reveal (the `RevealDecrypt` result),
/// or the field type directly for a plaintext one. (F08.)
fn reveal_return_ty(f: &IrFunction) -> Option<Ty> {
    for b in &f.blocks {
        if let Terminator::Return(Some(v)) = &b.terminator {
            if let Some(ty) = f.value_types.get(v) {
                if !matches!(ty, Ty::Unit) {
                    return Some(ty.clone());
                }
            }
        }
    }
    None
}

/// Canonical signature for a function: `name(type1,type2,...)`.
pub fn canonical_signature(name: &str, param_types: &[&Ty]) -> String {
    let parts: Vec<String> = param_types.iter().map(|t| abi_type_of(t)).collect();
    format!("{name}({})", parts.join(","))
}

/// Compute the full 32-byte event topic (keccak256 of the canonical event signature).
/// Unlike function selectors, event topics use all 32 bytes of the hash.
pub fn event_topic(signature: &str) -> [u8; 32] {
    let mut hasher = Keccak256::new();
    hasher.update(signature.as_bytes());
    hasher.finalize().into()
}

/// Build the canonical ABI signature string for an `IrEvent`,
/// e.g. `"Transfer(address,address,uint256)"`.
pub fn event_canonical_signature(event: &IrEvent) -> String {
    let parts: Vec<String> = event
        .params
        .iter()
        .map(|(_, ty, _)| abi_type_of(ty))
        .collect();
    format!("{}({})", event.name.name, parts.join(","))
}

/// Compute the 4-byte selector (first 4 bytes of keccak256(signature)).
pub fn selector(signature: &str) -> [u8; 4] {
    let mut hasher = Keccak256::new();
    hasher.update(signature.as_bytes());
    let out = hasher.finalize();
    let mut result = [0u8; 4];
    result.copy_from_slice(&out[..4]);
    result
}

/// Compute the 4-byte selector prefix for a precompile call (KSR-CVN-020).
///
/// Precompile calldata is prefixed with `keccak256("covenant.precompile.<name>:v1")[0..4]`
/// so that (a) each distinct operation is discriminated on the wire even if two
/// precompiles share an address on a future chain, and (b) versioning is a
/// property of the selector string, not an implicit coupling between compiler
/// and chain.
pub const PRECOMPILE_ABI_VERSION: u32 = 1;

pub fn precompile_selector(opcode_name: &str) -> [u8; 4] {
    let sig = format!("covenant.precompile.{opcode_name}:v{PRECOMPILE_ABI_VERSION}");
    selector(&sig)
}

/// Compute selectors for every public function in the module.
pub fn compute_selectors(
    module: &IrModule,
    include_test_actions: bool,
) -> Vec<(Box<str>, String, [u8; 4])> {
    let mut out = Vec::new();
    for f in &module.functions {
        if !is_emitted_function(f, include_test_actions) {
            continue;
        }
        let param_types: Vec<&Ty> = f.params.iter().map(|p| &p.ty).collect();
        let sig = canonical_signature(f.name.name.as_ref(), &param_types);
        let sel = selector(&sig);
        out.push((f.name.name.clone(), sig, sel));
    }
    out
}

/// Only action / view / reveal / selective_disclosure are part of the public
/// interface; `OnDestroy`, `Migrate`, `FieldInitializer`, `Test` are internal.
pub fn is_public_function(f: &IrFunction) -> bool {
    matches!(
        f.kind,
        IrFunctionKind::Action
            | IrFunctionKind::View
            | IrFunctionKind::Reveal
            | IrFunctionKind::SelectiveDisclosure
    )
}

/// As `is_public_function`, but keeps `Test` functions when the caller asked
/// for them (the `covenant test` harness needs to call them; a release build
/// must not expose them).
pub fn is_emitted_function(f: &IrFunction, include_test_actions: bool) -> bool {
    is_public_function(f) || (include_test_actions && matches!(f.kind, IrFunctionKind::Test))
}

/// Whether a function takes payment, i.e. whether its body reads `msg.value`.
///
/// Both halves of the artifact consult this, and they must agree. The ABI used
/// to hardcode `nonpayable` for every action while the runtime emitted no
/// CALLVALUE guard at all, so the published interface was a false statement
/// about the bytecode produced in the same invocation: `no_value(42)` accepted
/// 5 wei, and `deposit()`, whose body plainly reads `msg.value`, was declared
/// unpayable and so unreachable with value through its own published ABI. Now
/// codegen emits the CALLVALUE-must-be-zero guard for exactly the functions
/// this reports `false` for.
///
/// `view`/`reveal` stay read-only regardless: they are STATICCALLed, where
/// CALLVALUE is zero by construction.
pub fn function_is_payable(f: &IrFunction) -> bool {
    if matches!(
        f.kind,
        IrFunctionKind::View | IrFunctionKind::Reveal | IrFunctionKind::SelectiveDisclosure
    ) {
        return false;
    }
    f.blocks.iter().any(|b| {
        b.instructions
            .iter()
            .any(|i| matches!(i.opcode, Opcode::LoadMsgValue))
    })
}

/// Assemble the ABI JSON string.
pub fn emit_abi(module: &IrModule, include_test_actions: bool) -> String {
    let mut items: Vec<String> = Vec::new();

    // Functions.
    for f in &module.functions {
        if !is_emitted_function(f, include_test_actions) {
            continue;
        }
        let inputs: Vec<String> = f
            .params
            .iter()
            .map(|p| {
                format!(
                    "{{\"name\":\"{}\",\"type\":\"{}\"}}",
                    escape(p.name.name.as_ref()),
                    abi_type_of(&p.ty)
                )
            })
            .collect();
        // F08: a `reveal` is read-only and RETURNs 32 bytes at runtime, but the
        // one-liner form (`reveal total to owner`) carries no explicit `returns`
        // clause, so `f.returns` is `None` and the ABI used to advertise
        // `outputs:[]` — a caller decoding the return per the published ABI got
        // nothing. Recover the real output type from the return terminator's
        // value (the decrypted field type) when there is no declared return.
        let output_ty: Option<Ty> = match &f.returns {
            Some(ty) if !matches!(ty, Ty::Unit) => Some(ty.clone()),
            _ if matches!(f.kind, IrFunctionKind::Reveal) => reveal_return_ty(f),
            _ => None,
        };
        let outputs: Vec<String> = match &output_ty {
            Some(ty) => vec![format!(
                "{{\"name\":\"\",\"type\":\"{}\"}}",
                abi_type_of(ty)
            )],
            None => Vec::new(),
        };
        // F08: `reveal` is a read-only disclosure — it never writes state — so
        // its ABI `stateMutability` is `view`, not `nonpayable`.
        let mutability = match f.kind {
            IrFunctionKind::View => "view",
            IrFunctionKind::Reveal => "view",
            IrFunctionKind::SelectiveDisclosure => "view",
            // An action that reads `msg.value` is taking payment, so say so.
            // Declaring it `nonpayable` made it unreachable with value through
            // its own published interface: an ABI-driven client refuses to
            // attach value to a nonpayable entry.
            _ if function_is_payable(f) => "payable",
            _ => "nonpayable",
        };
        items.push(format!(
            "{{\"name\":\"{}\",\"type\":\"function\",\"inputs\":[{}],\"outputs\":[{}],\"stateMutability\":\"{}\"}}",
            escape(f.name.name.as_ref()),
            inputs.join(","),
            outputs.join(","),
            mutability
        ));
    }

    // Events.
    for e in &module.events {
        let inputs: Vec<String> = e
            .params
            .iter()
            .map(|(name, ty, indexed)| {
                format!(
                    "{{\"name\":\"{}\",\"type\":\"{}\",\"indexed\":{}}}",
                    escape(name.name.as_ref()),
                    abi_type_of(ty),
                    indexed
                )
            })
            .collect();
        items.push(format!(
            "{{\"name\":\"{}\",\"type\":\"event\",\"inputs\":[{}],\"anonymous\":false}}",
            escape(e.name.name.as_ref()),
            inputs.join(",")
        ));
    }

    // Errors.
    for err in &module.errors {
        let inputs: Vec<String> = err
            .params
            .iter()
            .enumerate()
            .map(|(i, ty)| format!("{{\"name\":\"p{i}\",\"type\":\"{}\"}}", abi_type_of(ty)))
            .collect();
        items.push(format!(
            "{{\"name\":\"{}\",\"type\":\"error\",\"inputs\":[{}]}}",
            escape(err.name.name.as_ref()),
            inputs.join(",")
        ));
    }

    format!("[{}]", items.join(","))
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
