//! End-to-end scenarios for SecretCoin (ERC-8227 confidential token).
//!
//! These tests exercise the ERC-8227 synthesis path end-to-end:
//! source → lex → parse → IR → stdlib lower (erc8227) → EVM bytecode →
//! mock interpreter. Ciphertext handles are minted/resolved via the
//! `MockPrecompileState` FHE mock, so tests remain deterministic.

use covenant_testing::{abi, CovenantTestHarness, U256};

const SECRET_COIN: &str =
    include_str!("../../covenant-lexer/tests/fixtures/example_06_secret_coin.cov");

fn setup() -> (CovenantTestHarness, covenant_testing::Contract) {
    let mut h = CovenantTestHarness::new();
    let c = h
        .deploy(SECRET_COIN, h.addrs.deployer)
        .expect("deploy SecretCoin");
    (h, c)
}

/// Mint a ciphertext for `amount` and return the 32-byte handle as U256 word.
fn mint_handle(h: &mut CovenantTestHarness, amount: u64) -> ([u8; 32], U256) {
    let handle = h.precompiles.mint_fhe(U256::from_u64(amount));
    let word = U256::from_be_bytes(handle);
    (handle, word)
}

/// Read the `balanceOfEncrypted` result for `holder` and resolve to plaintext.
fn read_encrypted_balance(
    h: &mut CovenantTestHarness,
    c: covenant_testing::Contract,
    holder: covenant_testing::Address,
) -> U256 {
    let handle_word = h.view_u256(
        c,
        h.addrs.alice,
        "balanceOfEncrypted(address)",
        &[holder.to_u256()],
    );
    let handle: [u8; 32] = handle_word.to_be_bytes();
    h.precompiles.resolve_fhe(&handle)
}

// ---------------------------------------------------------------------------
// Scenario 1: Deploy with encrypted supply
// ---------------------------------------------------------------------------

#[test]
fn secretcoin_deploys_with_encrypted_supply() {
    // After deploy, the deployer's encrypted balance should decrypt to the
    // genesis mint amount (1_000_000).
    let (mut h, c) = setup();
    let deployer = h.addrs.deployer;
    let plaintext = read_encrypted_balance(&mut h, c, deployer);
    assert_eq!(plaintext, U256::from_u64(1_000_000));
}

// ---------------------------------------------------------------------------
// Scenario 2: Encrypted transfer updates both balances
// ---------------------------------------------------------------------------

#[test]
fn secretcoin_encrypted_transfer_updates_balances() {
    let (mut h, c) = setup();
    // Mint a ciphertext for the transfer amount.
    let (_handle, amount_word) = mint_handle(&mut h, 250);

    // Deployer sends 250 (encrypted) to Alice.
    let _ = h.call_ok(
        c,
        h.addrs.deployer,
        "transferEncrypted(address,bytes32)",
        &[h.addrs.alice.to_u256(), amount_word],
    );

    let deployer = h.addrs.deployer;
    let alice = h.addrs.alice;
    let dep_balance = read_encrypted_balance(&mut h, c, deployer);
    let alice_balance = read_encrypted_balance(&mut h, c, alice);
    assert_eq!(dep_balance, U256::from_u64(1_000_000 - 250));
    assert_eq!(alice_balance, U256::from_u64(250));
}

// ---------------------------------------------------------------------------
// Scenario 3: Transfer with insufficient encrypted balance reverts
// ---------------------------------------------------------------------------

#[test]
fn secretcoin_encrypted_transfer_insufficient_reverts() {
    // Alice has zero balance. Sending any amount should revert via AssertEncrypted.
    let (mut h, c) = setup();
    let (_handle, amount_word) = mint_handle(&mut h, 1);

    let _data = h.call_revert(
        c,
        h.addrs.alice,
        "transferEncrypted(address,bytes32)",
        &[h.addrs.bob.to_u256(), amount_word],
    );
}

// ---------------------------------------------------------------------------
// Scenario 4: balanceOfEncrypted returns a resolvable handle
// ---------------------------------------------------------------------------

#[test]
fn secretcoin_balance_of_encrypted_returns_handle() {
    let (mut h, c) = setup();
    // The return value of balanceOfEncrypted is a bytes32 handle, not plaintext.
    let handle_word = h.view_u256(
        c,
        h.addrs.alice,
        "balanceOfEncrypted(address)",
        &[h.addrs.deployer.to_u256()],
    );
    // The handle itself must be non-zero (a minted FHE handle, not the zero sentinel).
    assert_ne!(handle_word, U256::ZERO);
}

// ---------------------------------------------------------------------------
// Scenario 5: TransferEncrypted event is emitted
// ---------------------------------------------------------------------------

#[test]
fn secretcoin_transfer_encrypted_event_emitted() {
    let (mut h, c) = setup();
    let (_handle, amount_word) = mint_handle(&mut h, 10);

    let _ = h.call_ok(
        c,
        h.addrs.deployer,
        "transferEncrypted(address,bytes32)",
        &[h.addrs.alice.to_u256(), amount_word],
    );
    assert!(
        h.find_event("TransferEncrypted(address,address,bytes32)")
            .is_some(),
        "expected TransferEncrypted event"
    );
}

// ---------------------------------------------------------------------------
// Scenario 6: approveEncrypted + allowanceEncrypted round-trip
// ---------------------------------------------------------------------------

#[test]
fn secretcoin_approve_encrypted_and_allowance_roundtrip() {
    let (mut h, c) = setup();
    let (handle_bytes, handle_word) = mint_handle(&mut h, 500);

    // Alice approves Bob for 500 (encrypted).
    let ret = h.call_ok(
        c,
        h.addrs.alice,
        "approveEncrypted(address,bytes32)",
        &[h.addrs.bob.to_u256(), handle_word],
    );
    // Returns `true` → last byte = 1.
    assert_eq!(ret.last().copied(), Some(1u8));

    // allowanceEncrypted(alice, bob) should return a handle resolving to 500.
    let alw_word = h.view_u256(
        c,
        h.addrs.alice,
        "allowanceEncrypted(address,address)",
        &[h.addrs.alice.to_u256(), h.addrs.bob.to_u256()],
    );
    let alw_handle: [u8; 32] = alw_word.to_be_bytes();
    // The allowance handle must hold the same plaintext value.
    // (The mock stores the handle directly so the value resolves correctly.)
    let plaintext = h.precompiles.resolve_fhe(&alw_handle);
    assert_eq!(plaintext, U256::from_u64(500));

    let _ = handle_bytes; // silence unused warning
}

// ---------------------------------------------------------------------------
// Scenario 7: Selectors match the ERC-8227 canonical values
// ---------------------------------------------------------------------------

#[test]
fn secretcoin_erc8227_selectors_match_canonical() {
    let (h, c) = setup();
    let sels = &h.artifact(c).function_selectors;

    for (name, sig) in [
        ("transferEncrypted", "transferEncrypted(address,bytes32)"),
        ("balanceOfEncrypted", "balanceOfEncrypted(address)"),
        (
            "transferFromEncrypted",
            "transferFromEncrypted(address,address,bytes32)",
        ),
        ("approveEncrypted", "approveEncrypted(address,bytes32)"),
        ("allowanceEncrypted", "allowanceEncrypted(address,address)"),
        ("totalSupply", "totalSupply()"),
        ("decimals", "decimals()"),
        ("symbol", "symbol()"),
        ("name", "name()"),
    ] {
        assert_eq!(
            sels.get(name).copied(),
            Some(abi::selector(sig)),
            "selector mismatch for `{name}`"
        );
    }
}

// ---------------------------------------------------------------------------
// Bonus: metadata views work like standard ERC-20
// ---------------------------------------------------------------------------

#[test]
fn secretcoin_runtime_size_is_reasonable() {
    let (h, c) = setup();
    assert!(h.artifact(c).runtime_bytecode.len() < 24 * 1024);
}

#[test]
fn secretcoin_name_returns_secret_coin() {
    let (mut h, c) = setup();
    let name = h.view_string(c, h.addrs.alice, "name()", &[]);
    assert_eq!(name, "Secret Coin");
}

#[test]
fn secretcoin_symbol_returns_scoin() {
    let (mut h, c) = setup();
    let sym = h.view_string(c, h.addrs.alice, "symbol()", &[]);
    assert_eq!(sym, "SCOIN");
}

#[test]
fn secretcoin_decimals_returns_eighteen() {
    let (mut h, c) = setup();
    let d = h.view_u256(c, h.addrs.alice, "decimals()", &[]);
    assert_eq!(d.low_u64(), 18);
}

#[test]
fn secretcoin_totalsupply_is_plaintext_million() {
    let (mut h, c) = setup();
    let ts = h.view_u256(c, h.addrs.alice, "totalSupply()", &[]);
    assert_eq!(ts, U256::from_u64(1_000_000));
}

#[test]
fn secretcoin_unknown_selector_reverts() {
    let (mut h, c) = setup();
    let result = h.raw_call(
        c,
        h.addrs.alice,
        vec![0xca, 0xfe, 0xba, 0xbe],
        U256::ZERO,
        false,
    );
    assert!(result.is_revert(), "unknown selector must revert");
}

#[test]
fn secretcoin_approve_emits_approval_encrypted_event() {
    let (mut h, c) = setup();
    let (_, handle_word) = mint_handle(&mut h, 100);
    let _ = h.call_ok(
        c,
        h.addrs.alice,
        "approveEncrypted(address,bytes32)",
        &[h.addrs.bob.to_u256(), handle_word],
    );
    assert!(
        h.find_event("ApprovalEncrypted(address,address,bytes32)")
            .is_some(),
        "expected ApprovalEncrypted event"
    );
}

#[test]
fn secretcoin_two_sequential_transfers_compound() {
    let (mut h, c) = setup();
    let (_h1, w1) = mint_handle(&mut h, 100);
    let (_h2, w2) = mint_handle(&mut h, 50);
    let deployer = h.addrs.deployer;
    let alice = h.addrs.alice;

    let _ = h.call_ok(
        c,
        deployer,
        "transferEncrypted(address,bytes32)",
        &[alice.to_u256(), w1],
    );
    let _ = h.call_ok(
        c,
        deployer,
        "transferEncrypted(address,bytes32)",
        &[alice.to_u256(), w2],
    );

    let alice_bal = read_encrypted_balance(&mut h, c, alice);
    assert_eq!(alice_bal, U256::from_u64(150));
    let dep_bal = read_encrypted_balance(&mut h, c, deployer);
    assert_eq!(dep_bal, U256::from_u64(1_000_000 - 150));
}

#[test]
fn secretcoin_transfer_from_with_sufficient_allowance() {
    let (mut h, c) = setup();
    // Approve exactly the transfer amount to exercise the FheCmpGe (>=) path.
    let (_, allow_word) = mint_handle(&mut h, 200);
    let deployer = h.addrs.deployer;
    let alice = h.addrs.alice;
    let bob = h.addrs.bob;

    let _ = h.call_ok(
        c,
        deployer,
        "approveEncrypted(address,bytes32)",
        &[alice.to_u256(), allow_word],
    );
    // Alice transfers 200 == 200 allowance; FheCmpGe must return true.
    let (_, transfer_word) = mint_handle(&mut h, 200);
    let _ = h.call_ok(
        c,
        alice,
        "transferFromEncrypted(address,address,bytes32)",
        &[deployer.to_u256(), bob.to_u256(), transfer_word],
    );
    let bob_bal = read_encrypted_balance(&mut h, c, bob);
    assert_eq!(bob_bal, U256::from_u64(200));
}
