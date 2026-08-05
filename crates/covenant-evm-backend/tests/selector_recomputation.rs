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

/// EVERY key in the manifest's `selectors` block, paired with the signature
/// each one claims to be. This list is deliberately exhaustive: an earlier
/// version checked six of the twenty entries, which proved a subset correct
/// and said nothing about the rest. `manifest_is_fully_covered` below asserts
/// the count matches, so a new manifest entry cannot slip in unchecked.
///
/// `amnesiaSetupV091` is the one-argument overload; the unsuffixed
/// `amnesiaSetup` is the three-argument one. `proofAggregate` and
/// `pqKeygenFromSeed` exist in the deployed helpers and the manifest but have
/// no Covenant opcode, so they are checked here for selector correctness but
/// are absent from the dispatch table by design.
const MANIFEST: &[(&str, &str)] = &[
    ("amnesiaSetup", "amnesiaSetup(uint256,uint256,uint256)"),
    ("amnesiaSetupV091", "amnesiaSetup(uint256)"),
    ("amnesiaSubmitShare", "amnesiaSubmitShare(uint256,bytes32)"),
    ("amnesiaFinalize", "amnesiaFinalize(uint256)"),
    ("amnesiaDestroy", "amnesiaDestroy(uint256)"),
    ("encryptTrivial", "encryptTrivial(uint256)"),
    ("encryptFresh", "encryptFresh(uint256,uint256)"),
    ("add", "add(bytes32,bytes32)"),
    ("sub", "sub(bytes32,bytes32)"),
    ("mul", "mul(bytes32,bytes32)"),
    ("eq", "eq(bytes32,bytes32)"),
    ("lt", "lt(bytes32,bytes32)"),
    ("cmux", "cmux(bytes32,bytes32,bytes32)"),
    ("decrypt", "decrypt(bytes32,address)"),
    ("verify", "verify(bytes32,bytes,bytes)"),
    ("nullifier", "nullifier(bytes32)"),
    ("proofAggregate", "proofAggregate(bytes)"),
    ("pqVerify", "pqVerify(bytes32,bytes,bytes)"),
    ("pqKeygenFromSeed", "pqKeygenFromSeed(uint256)"),
    ("pqRandom", "pqRandom(uint256)"),
];

/// Manifest keys that intentionally have no Covenant opcode. Present in the
/// deployed helpers and worth verifying, but no compiler dispatch reaches
/// them, so they are excluded from the dispatch cross-check.
const MANIFEST_WITHOUT_OPCODE: &[&str] = &["proofAggregate", "pqKeygenFromSeed"];

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

/// The MANIFEST list must name every selector the manifest publishes.
///
/// The point of the checks above is only as strong as their coverage. If the
/// manifest gains a selector this list does not, the new one goes unverified,
/// which is exactly the state the six-of-twenty version was in. Counting both
/// ways closes that: no manifest entry unlisted, no listed entry absent from
/// the manifest.
#[test]
fn manifest_is_fully_covered() {
    // Count the entries in the manifest's `selectors` block by finding the
    // block and counting `"key": "0x...."` pairs inside it.
    let block_start = REGISTRY_JSON
        .find("\"selectors\"")
        .expect("manifest has no selectors block");
    let after = &REGISTRY_JSON[block_start..];
    let open = after
        .find('{')
        .expect("selectors block has no opening brace");
    let close = after[open..]
        .find('}')
        .expect("selectors block has no closing brace")
        + open;
    let block = &after[open..close];
    let published: std::collections::BTreeSet<&str> = block
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            let key = l.strip_prefix('"')?;
            let end = key.find('"')?;
            // Only lines that actually assign a 0x selector.
            if l.contains("0x") {
                Some(&key[..end])
            } else {
                None
            }
        })
        .collect();

    let listed: std::collections::BTreeSet<&str> = MANIFEST.iter().map(|(k, _)| *k).collect();

    let unlisted: Vec<_> = published.difference(&listed).collect();
    let missing: Vec<_> = listed.difference(&published).collect();

    assert!(
        unlisted.is_empty(),
        "the manifest publishes selectors this test never checks: {unlisted:?}. \
         Add them to MANIFEST so they are recomputed."
    );
    assert!(
        missing.is_empty(),
        "MANIFEST names selectors that are no longer in the manifest: {missing:?}"
    );
    assert_eq!(
        published.len(),
        MANIFEST.len(),
        "manifest has {} selectors, MANIFEST checks {}",
        published.len(),
        MANIFEST.len()
    );
}

/// Every opcode the dispatch table knows must be represented in the manifest,
/// except the two the manifest carries that have no opcode. This is the other
/// direction: a compiler dispatch entry absent from the published manifest
/// would mean the compiler calls a method an operator cannot verify.
#[test]
fn every_dispatched_opcode_appears_in_the_manifest() {
    // The opcode-to-manifest-key pairing, for the entries that have both.
    let opcode_to_key = [
        ("AmnesiaBegin", "amnesiaSetupV091"),
        ("AmnesiaSubmitShare", "amnesiaSubmitShare"),
        ("AmnesiaFinalize", "amnesiaFinalize"),
        ("DestructionProof", "amnesiaDestroy"),
        ("FheEncryptTrivial", "encryptTrivial"),
        ("FheEncryptFresh", "encryptFresh"),
        ("FheAdd", "add"),
        ("FheSub", "sub"),
        ("FheMul", "mul"),
        ("FheCmpEq", "eq"),
        ("FheCmpLt", "lt"),
        ("FheSelect", "cmux"),
        ("RevealDecrypt", "decrypt"),
        ("ZkVerify", "verify"),
        ("ZkNullifier", "nullifier"),
        ("PqVerifyDilithium", "pqVerify"),
        ("PqRand", "pqRandom"),
    ];
    let mut wrong = Vec::new();
    for (opcode, key) in opcode_to_key {
        let table = hex4(
            helper_selector_for_opcode(opcode)
                .unwrap_or_else(|| panic!("`{opcode}` is not in the dispatch table")),
        );
        match manifest_selector(key) {
            None => wrong.push(format!("{opcode}: manifest has no `{key}`")),
            Some(m) if m != table => {
                wrong.push(format!("{opcode} / {key}: table {table}, manifest {m}"))
            }
            Some(_) => {}
        }
    }
    // The manifest-only entries are documented, not accidental.
    for key in MANIFEST_WITHOUT_OPCODE {
        assert!(
            manifest_selector(key).is_some(),
            "`{key}` was declared opcode-less but is not in the manifest either"
        );
    }
    assert!(
        wrong.is_empty(),
        "the compiler dispatches to methods the manifest does not correctly \
         publish:\n  {}",
        wrong.join("\n  ")
    );
}

/// The wire name is not the Rust name, and completing the rename would
/// silently change emitted bytecode.
///
/// `Opcode::DestructionCommitment` reports `stable_name() == "DestructionProof"`
/// on purpose. That string is hashed into
/// `covenant.precompile.<name>:v<PRECOMPILE_ABI_VERSION>` to build the
/// calldata prefix of every emitted precompile call, so renaming it changes
/// the bytecode of already-published artifacts. It moves when the ABI version
/// moves, with the helper redeployment.
///
/// Someone will eventually notice the mismatch and tidy it up. This makes
/// that a test failure rather than a silent protocol break.
#[test]
fn the_wire_name_of_the_destruction_opcode_has_not_moved() {
    use covenant_ir::Opcode;
    assert_eq!(
        Opcode::DestructionCommitment.stable_name(),
        "DestructionProof",
        "the wire name changed. If that is deliberate, bump \
         PRECOMPILE_ABI_VERSION in the same commit and republish the helper \
         suite, because every emitted precompile call prefix just moved."
    );
    assert!(
        helper_selector_for_opcode("DestructionProof").is_some(),
        "the dispatch table no longer answers to the wire name, so helper \
         calls for this opcode would fall through to E520"
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
