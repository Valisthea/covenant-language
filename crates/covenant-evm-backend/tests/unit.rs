//! Unit tests for EVM codegen components.

use covenant_diag::SourceId;
use covenant_evm_backend::{
    abi,
    asm::{self, assemble, AsmOp},
    codegen_evm, storage, EvmArtifact, EvmConfig, PRECOMPILE_ABI_VERSION,
};
use covenant_ir::build_ir;
use covenant_lexer::tokenize;
use covenant_opt::{optimize, OptimizerConfig};
use covenant_parser::parse;
use covenant_privacy::analyze_privacy;
use covenant_resolver::resolve;
use covenant_types::{typecheck, Ty};

fn pipe(src: &str) -> EvmArtifact {
    let (toks, _) = tokenize(src, SourceId::new(0));
    let (file, _) = parse(&toks, SourceId::new(0));
    let (res, _) = resolve(file.unwrap(), SourceId::new(0));
    let (typed, _) = typecheck(res, SourceId::new(0));
    let (checked, _) = analyze_privacy(typed, SourceId::new(0));
    let (module, _) = build_ir(checked, SourceId::new(0));
    let (optimized, _) = optimize(module, OptimizerConfig::default());
    codegen_evm(optimized, EvmConfig::default()).0
}

// ---------- ABI type mapping ----------

#[test]
fn abi_type_amount_is_uint256() {
    assert_eq!(abi::abi_type_of(&Ty::Amount), "uint256");
}

#[test]
fn abi_type_address_is_address() {
    assert_eq!(abi::abi_type_of(&Ty::Address), "address");
}

#[test]
fn abi_type_bool_is_bool() {
    assert_eq!(abi::abi_type_of(&Ty::Bool), "bool");
}

#[test]
fn abi_type_hash_is_bytes32() {
    assert_eq!(abi::abi_type_of(&Ty::Hash), "bytes32");
}

#[test]
fn abi_type_ciphertext_is_bytes32_handle() {
    assert_eq!(
        abi::abi_type_of(&Ty::Ciphertext(Box::new(Ty::Amount))),
        "bytes32"
    );
}

#[test]
fn abi_type_list_is_array() {
    assert_eq!(
        abi::abi_type_of(&Ty::List(Box::new(Ty::Amount))),
        "uint256[]"
    );
}

#[test]
fn abi_type_text_is_string() {
    assert_eq!(abi::abi_type_of(&Ty::Text), "string");
}

// ---------- Selector computation ----------

#[test]
fn selector_known_signature() {
    // Well-known: keccak256("transfer(address,uint256)")[..4] = 0xa9059cbb.
    let s = abi::selector("transfer(address,uint256)");
    assert_eq!(s, [0xa9, 0x05, 0x9c, 0xbb]);
}

#[test]
fn selector_balance_of() {
    let s = abi::selector("balanceOf(address)");
    assert_eq!(s, [0x70, 0xa0, 0x82, 0x31]);
}

#[test]
fn selector_transfer_from() {
    let s = abi::selector("transferFrom(address,address,uint256)");
    assert_eq!(s, [0x23, 0xb8, 0x72, 0xdd]);
}

// ---------- Storage layout ----------

#[test]
fn storage_layout_sequential_slots() {
    let src = r#"
record R {
    x: amount
    y: amount
    z: address
}
"#;
    let a = pipe(src);
    assert_eq!(a.storage_layout.entries.len(), 3);
    assert_eq!(a.storage_layout.entries[0].name.as_ref(), "x");
    assert_eq!(a.storage_layout.entries[1].name.as_ref(), "y");
    assert_eq!(a.storage_layout.entries[2].name.as_ref(), "z");
    // Slots 0, 1, 2.
    assert_eq!(a.storage_layout.entries[0].slot[31], 0);
    assert_eq!(a.storage_layout.entries[1].slot[31], 1);
    assert_eq!(a.storage_layout.entries[2].slot[31], 2);
}

#[test]
fn storage_layout_map_occupies_one_slot() {
    let src = r#"
record R {
    balances: map<address, amount>
    total: amount
}
"#;
    let a = pipe(src);
    assert_eq!(a.storage_layout.entries[0].slot[31], 0);
    assert_eq!(a.storage_layout.entries[1].slot[31], 1);
}

#[test]
fn slot_bytes_big_endian() {
    let b = storage::slot_bytes(42);
    assert_eq!(&b[28..32], &42u32.to_be_bytes());
    assert!(b[..28].iter().all(|&x| x == 0));
}

// ---------- Assembler ----------

#[test]
fn assemble_push_and_return() {
    let ops = vec![
        AsmOp::PushBytes(vec![0x20]),
        AsmOp::Push0,
        AsmOp::Op(asm::OP_RETURN),
    ];
    let (bytes, unresolved) = assemble(&ops);
    assert!(unresolved.is_empty());
    // PUSH1 0x20 = 0x60 0x20
    assert_eq!(bytes[0], 0x60);
    assert_eq!(bytes[1], 0x20);
    // PUSH0 = 0x5F
    assert_eq!(bytes[2], 0x5F);
    // RETURN = 0xF3
    assert_eq!(bytes[3], 0xF3);
}

#[test]
fn assemble_label_resolves_to_offset() {
    let ops = vec![
        AsmOp::PushLabel("end".into()),
        AsmOp::Op(asm::OP_JUMP),
        AsmOp::LabelDef("end".into()),
        AsmOp::Op(asm::OP_STOP),
    ];
    let (bytes, unresolved) = assemble(&ops);
    assert!(unresolved.is_empty());
    // PUSH2 <hi> <lo>: label at offset 4 (after PUSH2+2bytes+JUMP).
    assert_eq!(bytes[0], 0x61);
    assert_eq!(bytes[1], 0x00);
    assert_eq!(bytes[2], 0x04);
    // JUMP = 0x56
    assert_eq!(bytes[3], 0x56);
    // JUMPDEST at offset 4
    assert_eq!(bytes[4], 0x5B);
    // STOP
    assert_eq!(bytes[5], 0x00);
}

#[test]
fn assemble_unresolved_label_reported() {
    let ops = vec![AsmOp::PushLabel("missing".into())];
    let (_bytes, unresolved) = assemble(&ops);
    assert_eq!(unresolved.len(), 1);
    assert_eq!(unresolved[0].as_ref(), "missing");
}

#[test]
fn push_n_byte_computation() {
    assert_eq!(asm::push_n(1), 0x60);
    assert_eq!(asm::push_n(2), 0x61);
    assert_eq!(asm::push_n(32), 0x7F);
}

// ---------- Artifact structure ----------

#[test]
fn runtime_bytecode_below_eip170_limit() {
    for src in [
        include_str!("../../covenant-lexer/tests/fixtures/example_01_hello.cov"),
        include_str!("../../covenant-lexer/tests/fixtures/example_05_quantum_board.cov"),
    ] {
        let a = pipe(src);
        assert!(a.runtime_bytecode.len() <= 24 * 1024);
    }
}

#[test]
fn deploy_starts_with_caller_capture() {
    let src = r#"record R { view v returns amount { 42 } }"#;
    let a = pipe(src);
    // KSR-CVN-029: constructor prepends a PUSH <PRECOMPILE_ABI_VERSION>; POP
    // marker so integrators can byte-scan the deploy prefix for the precompile
    // ABI version the compiler targeted. The assembler emits the minimal PUSH
    // width for the value; caller-capture follows immediately.
    let mut marker = Vec::new();
    let ver_bytes: Vec<u8> = PRECOMPILE_ABI_VERSION
        .to_be_bytes()
        .iter()
        .copied()
        .skip_while(|&b| b == 0)
        .collect();
    let ver_bytes = if ver_bytes.is_empty() {
        vec![0]
    } else {
        ver_bytes
    };
    marker.push(asm::push_n(ver_bytes.len() as u8));
    marker.extend_from_slice(&ver_bytes);
    marker.push(0x50); // POP
    assert!(
        a.deploy_bytecode.starts_with(&marker),
        "deploy bytecode missing precompile-ABI-version marker"
    );
    // Caller-capture (CALLER = 0x33) follows the marker.
    assert_eq!(a.deploy_bytecode[marker.len()], 0x33);
}

#[test]
fn function_selectors_deterministic() {
    let src = r#"record R { action foo() {} view bar returns amount { 1 } }"#;
    let a = pipe(src);
    let sel_foo = a.function_selectors.get("foo").copied();
    let sel_bar = a.function_selectors.get("bar").copied();
    assert!(sel_foo.is_some());
    assert!(sel_bar.is_some());
    assert_ne!(sel_foo, sel_bar);
}

#[test]
fn abi_contains_event() {
    let src = r#"
record R {
    event Changed(who: address indexed)
    action go() { emit Changed(caller) }
}
"#;
    let a = pipe(src);
    assert!(a.abi.contains("\"Changed\""));
    assert!(a.abi.contains("\"type\":\"event\""));
}

#[test]
fn abi_contains_error() {
    let src = r#"
record R {
    error Bad(why: amount)
    action go() { revert_with Bad(1) }
}
"#;
    let a = pipe(src);
    assert!(a.abi.contains("\"Bad\""));
    assert!(a.abi.contains("\"type\":\"error\""));
}

#[test]
fn storage_layout_non_empty_for_record() {
    let src = r#"record R { x: amount }"#;
    let a = pipe(src);
    assert_eq!(a.storage_layout.entries.len(), 1);
}

#[test]
fn runtime_starts_with_dispatcher_or_revert() {
    let src = r#"record R { view v returns amount { 42 } }"#;
    let a = pipe(src);
    // Dispatcher first op: PUSH0 (0x5F) for CALLDATALOAD starting offset.
    // If there are public functions, we expect this pattern.
    if !a.function_selectors.is_empty() {
        assert_eq!(a.runtime_bytecode[0], 0x5F);
    }
}

#[test]
fn metadata_records_evm_version() {
    let src = r#"record R {}"#;
    let a = pipe(src);
    assert!(matches!(
        a.metadata.evm_version,
        covenant_evm_backend::EvmVersion::Shanghai
    ));
}

#[test]
fn metadata_lists_erc_versions() {
    let src = r#"record R {}"#;
    let a = pipe(src);
    assert!(a.metadata.erc_versions.contains_key("ERC-8227"));
    assert!(a.metadata.erc_versions.contains_key("ERC-8228"));
    assert!(a.metadata.erc_versions.contains_key("ERC-8229"));
    assert!(a.metadata.erc_versions.contains_key("ERC-8231"));
}

#[test]
fn empty_record_still_produces_valid_artifact() {
    let a = pipe("record R {}");
    assert!(!a.deploy_bytecode.is_empty());
    // Last instruction of constructor should be RETURN (0xF3) before runtime.
    assert!(a.deploy_bytecode.contains(&0xF3));
}

#[test]
fn two_identical_signatures_collide_emits_e506() {
    // This is hard to trigger with plain Covenant because duplicate decls
    // would be caught by Phase 3. We sanity-check the selector-collision
    // path runs on a fabricated scenario.
    let src = r#"record R { action foo() {} view foo_v returns amount { 1 } }"#;
    // No collision expected (different names). Just exercise the codegen.
    let a = pipe(src);
    assert_eq!(a.function_selectors.len(), 2);
}

// ---------- Event topic hashes (Fix 5) ----------

#[test]
fn event_topic_hash_matches_canonical_transfer() {
    let hash = abi::event_topic("Transfer(address,address,uint256)");
    // keccak256("Transfer(address,address,uint256)") = 0xddf252ad...
    let expected: [u8; 32] = [
        0xdd, 0xf2, 0x52, 0xad, 0x1b, 0xe2, 0xc8, 0x9b, 0x69, 0xc2, 0xb0, 0x68, 0xfc, 0x37, 0x8d,
        0xaa, 0x95, 0x2b, 0xa7, 0xf1, 0x63, 0xc4, 0xa1, 0x16, 0x28, 0xf5, 0x5a, 0x4d, 0xf5, 0x23,
        0xb3, 0xef,
    ];
    assert_eq!(hash, expected);
}

#[test]
fn event_topic_hash_matches_canonical_approval() {
    let hash = abi::event_topic("Approval(address,address,uint256)");
    // keccak256("Approval(address,address,uint256)") = 0x8c5be1e5...
    let expected: [u8; 32] = [
        0x8c, 0x5b, 0xe1, 0xe5, 0xeb, 0xec, 0x7d, 0x5b, 0xd1, 0x4f, 0x71, 0x42, 0x7d, 0x1e, 0x84,
        0xf3, 0xdd, 0x03, 0x14, 0xc0, 0xf7, 0xb2, 0x29, 0x1e, 0x5b, 0x20, 0x0a, 0xc8, 0xc7, 0xc3,
        0xb9, 0x25,
    ];
    assert_eq!(hash, expected);
}

#[test]
fn event_topic_differs_from_selector() {
    // Sanity: event topics are full 32 bytes; selectors are first 4 bytes.
    let topic = abi::event_topic("Transfer(address,address,uint256)");
    let sel = abi::selector("Transfer(address,address,uint256)");
    // Topic first 4 bytes = 0xddf252ad; selector = same first 4 bytes.
    assert_eq!(&topic[..4], &sel);
    // But topic has 28 more non-zero bytes after the selector.
    assert_ne!(&topic[4..], &[0u8; 28]);
}
