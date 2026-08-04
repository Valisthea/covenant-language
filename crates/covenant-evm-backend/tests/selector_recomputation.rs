//! Every helper selector must be recomputable from its canonical signature.
//!
//! The dispatch table in `target.rs` and the published manifest in
//! `config/helper-addresses-v0.9.0.json` were two hand-maintained lists of the
//! same facts, and they had drifted: the manifest recorded `amnesiaSetupV091`
//! as `0xa1c83bd6`, which is the selector of no signature at all.
//! `keccak256("amnesiaSetup(uint256)")` is `0x09dc3eb0`, which is what the
//! compiler emits. The compiler was right and the document an operator would
//! read was wrong, which is the worse way round.
//!
//! `registry_consistency.rs` could not catch it. It scans for four addresses
//! inside an 80-character window and compares a version string, so a wrong
//! selector next to a right address passes. Nothing anywhere in the suite
//! recomputed a selector.
//!
//! This does. Every entry is derived from its signature at test time, so a
//! hand-typed value can never be accepted just because it looks plausible.

use covenant_evm_backend::abi;
use covenant_evm_backend::target::helper_selector_for_opcode;

const REGISTRY_JSON: &str = include_str!("../../../config/helper-addresses-v0.9.0.json");

/// Opcode name as the compiler knows it, paired with the canonical Solidity
/// signature it is supposed to call. The signature is the source of truth;
/// the four bytes in `target.rs` are derived from it and must agree.
const DISPATCH: &[(&str, &str)] = &[
    // CeremonyHelper.sol
    ("AmnesiaBegin", "amnesiaSetup(uint256)"),
    ("AmnesiaSubmitShare", "amnesiaSubmitShare(uint256,bytes32)"),
    ("AmnesiaFinalize", "amnesiaFinalize(uint256)"),
    ("DestructionProof", "amnesiaDestroy(uint256)"),
    // MockedFHEHelper.sol
    ("FheEncryptTrivial", "encryptTrivial(uint256)"),
    ("FheEncryptFresh", "encryptFresh(uint256,uint256)"),
    ("FheAdd", "add(bytes32,bytes32)"),
    ("FheSub", "sub(bytes32,bytes32)"),
    ("FheMul", "mul(bytes32,bytes32)"),
    ("FheCmpEq", "eq(bytes32,bytes32)"),
    ("FheCmpLt", "lt(bytes32,bytes32)"),
    ("FheSelect", "cmux(bytes32,bytes32,bytes32)"),
    ("RevealDecrypt", "decrypt(bytes32,address)"),
    // MockedZKVerifier.sol
    ("ZkVerify", "verify(bytes32,bytes,bytes)"),
    ("ZkNullifier", "nullifier(bytes32)"),
    // MockedPQVerifier.sol
    ("PqVerifyDilithium", "pqVerify(bytes32,bytes,bytes)"),
    ("PqRand", "pqRandom(uint256)"),
];

/// Keys in the manifest's `selectors` block, paired with the signature each
/// one claims to be. `amnesiaSetupV091` is the one-argument overload; the
/// unsuffixed `amnesiaSetup` is the three-argument one.
const MANIFEST: &[(&str, &str)] = &[
    ("amnesiaSetup", "amnesiaSetup(uint256,uint256,uint256)"),
    ("amnesiaSetupV091", "amnesiaSetup(uint256)"),
    ("amnesiaSubmitShare", "amnesiaSubmitShare(uint256,bytes32)"),
    ("amnesiaFinalize", "amnesiaFinalize(uint256)"),
    ("amnesiaDestroy", "amnesiaDestroy(uint256)"),
    ("encryptTrivial", "encryptTrivial(uint256)"),
];

fn hex4(sel: [u8; 4]) -> String {
    format!("0x{:02x}{:02x}{:02x}{:02x}", sel[0], sel[1], sel[2], sel[3])
}

/// Pull `"key": "0x........"` out of the manifest without a JSON dependency.
fn manifest_selector(key: &str) -> Option<String> {
    let marker = format!("\"{key}\"");
    let at = REGISTRY_JSON.find(&marker)? + marker.len();
    let rest = &REGISTRY_JSON[at..];
    let open = rest.find("0x")?;
    // A selector is `0x` plus 8 hex digits. Anything longer is a different
    // field and this key was not what we thought it was.
    let candidate = &rest[open..];
    if candidate.len() < 10 {
        return None;
    }
    let value = &candidate[..10];
    if !value[2..].chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(value.to_ascii_lowercase())
}

#[test]
fn every_dispatch_selector_matches_its_signature() {
    let mut wrong = Vec::new();
    for (opcode, signature) in DISPATCH {
        let expected = abi::selector(signature);
        let actual = helper_selector_for_opcode(opcode)
            .unwrap_or_else(|| panic!("`{opcode}` has no entry in the dispatch table"));
        if actual != expected {
            wrong.push(format!(
                "{opcode}: table says {}, keccak256({signature:?}) is {}",
                hex4(actual),
                hex4(expected)
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "the compiler would call the wrong function:\n  {}",
        wrong.join("\n  ")
    );
}

#[test]
fn every_manifest_selector_matches_its_signature() {
    let mut wrong = Vec::new();
    for (key, signature) in MANIFEST {
        let expected = hex4(abi::selector(signature));
        match manifest_selector(key) {
            None => wrong.push(format!("{key}: no selector found in the manifest")),
            Some(found) if found != expected => wrong.push(format!(
                "{key}: manifest says {found}, keccak256({signature:?}) is {expected}"
            )),
            Some(_) => {}
        }
    }
    assert!(
        wrong.is_empty(),
        "the published manifest disagrees with the signatures it names, so an \
         operator verifying a deployment against it reaches a false \
         conclusion:\n  {}",
        wrong.join("\n  ")
    );
}

/// The two lists describe the same functions and must not diverge, which is
/// the failure that actually happened.
#[test]
fn the_dispatch_table_and_the_manifest_agree() {
    let pairs = [
        ("AmnesiaBegin", "amnesiaSetupV091"),
        ("AmnesiaSubmitShare", "amnesiaSubmitShare"),
        ("AmnesiaFinalize", "amnesiaFinalize"),
        ("DestructionProof", "amnesiaDestroy"),
        ("FheEncryptTrivial", "encryptTrivial"),
    ];
    let mut wrong = Vec::new();
    for (opcode, key) in pairs {
        let from_table = hex4(helper_selector_for_opcode(opcode).unwrap());
        let from_manifest = manifest_selector(key).unwrap_or_else(|| {
            panic!("`{key}` is missing from the manifest");
        });
        if from_table != from_manifest {
            wrong.push(format!(
                "{opcode} / {key}: table {from_table}, manifest {from_manifest}"
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "the compiler and the published manifest name different functions:\n  {}",
        wrong.join("\n  ")
    );
}

/// A negative control. If `abi::selector` returned a constant, or if the
/// comparison were vacuous, the three tests above would pass on anything.
#[test]
fn the_recomputation_used_above_does_discriminate() {
    let a = abi::selector("amnesiaSetup(uint256)");
    let b = abi::selector("amnesiaSetup(uint256,uint256,uint256)");
    assert_ne!(
        a, b,
        "two different signatures produced the same selector, so these tests \
         prove nothing"
    );
    // The exact value that was wrong in the manifest, so this test fails if
    // anyone reintroduces it.
    assert_eq!(hex4(a), "0x09dc3eb0");
    assert_ne!(hex4(a), "0xa1c83bd6");
}
