//! End-to-end scenarios for the Coin (ERC-20) Basics example.
//!
//! These tests exercise the compiler's ERC-20 synthesis path end-to-end:
//! source → lex → parse → resolve → typecheck → privacy → IR → stdlib lower →
//! optimize → EVM bytecode → mock interpreter. Each scenario drives the
//! resulting contract through its standard ABI and asserts observable
//! behavior (state, events, reverts).
//!
//! After Fix 1 (calldata → params) and Fix 2 (genesis mint), Coin is a
//! real ERC-20: the deployer starts with `supply` tokens, transfer moves
//! balances, and approve/allowance round-trip.

use covenant_testing::{abi::event_topic, CovenantTestHarness, U256};

const COIN: &str = include_str!("../../covenant-lexer/tests/fixtures/example_02_coin.cov");

fn setup() -> (CovenantTestHarness, covenant_testing::Contract) {
    let mut h = CovenantTestHarness::new();
    let c = h.deploy(COIN, h.addrs.deployer).expect("deploy coin");
    (h, c)
}

// ---------------------------------------------------------------------------
// Metadata view functions
// ---------------------------------------------------------------------------

#[test]
fn deploys_successfully() {
    let (h, c) = setup();
    assert!(!h.artifact(c).runtime_bytecode.is_empty());
}

#[test]
fn runtime_size_is_reasonable() {
    let (h, c) = setup();
    assert!(h.artifact(c).runtime_bytecode.len() < 24 * 1024);
}

#[test]
fn selector_map_has_nine_functions() {
    let (h, c) = setup();
    let sels = &h.artifact(c).function_selectors;
    for expected in [
        "totalSupply",
        "balanceOf",
        "transfer",
        "transferFrom",
        "approve",
        "allowance",
        "decimals",
        "symbol",
        "name",
    ] {
        assert!(sels.contains_key(expected), "missing {expected}");
    }
}

#[test]
fn decimals_returns_eighteen() {
    let (mut h, c) = setup();
    let v = h.view_u256(c, h.addrs.alice, "decimals()", &[]);
    assert_eq!(v.low_u64(), 18);
}

#[test]
fn totalsupply_matches_declared_supply() {
    // Fix 2: `supply: 1_000_000 to deployer` seeds total_supply in the
    // constructor.
    let (mut h, c) = setup();
    let v = h.view_u256(c, h.addrs.alice, "totalSupply()", &[]);
    assert_eq!(v, U256::from_u64(1_000_000));
}

#[test]
fn balanceof_deployer_equals_supply() {
    // Fix 2: the deployer is the genesis-mint recipient.
    let (mut h, c) = setup();
    let v = h.view_u256(
        c,
        h.addrs.alice,
        "balanceOf(address)",
        &[h.addrs.deployer.to_u256()],
    );
    assert_eq!(v, U256::from_u64(1_000_000));
}

#[test]
fn balanceof_is_zero_for_untouched_account() {
    let (mut h, c) = setup();
    let v = h.view_u256(
        c,
        h.addrs.alice,
        "balanceOf(address)",
        &[h.addrs.bob.to_u256()],
    );
    assert_eq!(v, U256::ZERO);
}

#[test]
fn allowance_is_zero_for_untouched_pair() {
    let (mut h, c) = setup();
    let v = h.view_u256(
        c,
        h.addrs.alice,
        "allowance(address,address)",
        &[h.addrs.alice.to_u256(), h.addrs.bob.to_u256()],
    );
    assert_eq!(v, U256::ZERO);
}

// ---------------------------------------------------------------------------
// Transfer / approve happy and sad paths
// ---------------------------------------------------------------------------

#[test]
fn transfer_from_deployer_moves_balance() {
    // Fix 1 + Fix 2: deployer has 1_000_000 at genesis, can now transfer.
    let (mut h, c) = setup();
    let _ = h.call_ok(
        c,
        h.addrs.deployer,
        "transfer(address,uint256)",
        &[h.addrs.alice.to_u256(), U256::from_u64(100)],
    );
    let alice_bal = h.view_u256(
        c,
        h.addrs.alice,
        "balanceOf(address)",
        &[h.addrs.alice.to_u256()],
    );
    assert_eq!(alice_bal, U256::from_u64(100));
    let dep_bal = h.view_u256(
        c,
        h.addrs.alice,
        "balanceOf(address)",
        &[h.addrs.deployer.to_u256()],
    );
    assert_eq!(dep_bal, U256::from_u64(1_000_000 - 100));
}

#[test]
fn transfer_beyond_balance_reverts() {
    // Alice still has zero — sending from her must revert.
    let (mut h, c) = setup();
    let _data = h.call_revert(
        c,
        h.addrs.alice,
        "transfer(address,uint256)",
        &[h.addrs.bob.to_u256(), U256::from_u64(1)],
    );
}

#[test]
fn transfer_emits_transfer_event() {
    let (mut h, c) = setup();
    let _ = h.call_ok(
        c,
        h.addrs.deployer,
        "transfer(address,uint256)",
        &[h.addrs.alice.to_u256(), U256::from_u64(42)],
    );
    assert!(h.find_event("Transfer(address,address,uint256)").is_some());
}

#[test]
fn transfer_event_has_canonical_topic_and_three_topics() {
    let (mut h, c) = setup();
    let _ = h.call_ok(
        c,
        h.addrs.deployer,
        "transfer(address,uint256)",
        &[h.addrs.alice.to_u256(), U256::from_u64(77)],
    );
    let log = h
        .find_event("Transfer(address,address,uint256)")
        .expect("Transfer event emitted");
    // topic0 must be the canonical 32-byte keccak hash
    assert_eq!(
        log.topics[0],
        event_topic("Transfer(address,address,uint256)")
    );
    // LOG3: event sig + from (indexed) + to (indexed) = 3 topics
    assert_eq!(log.topics.len(), 3, "Transfer must use LOG3");
    // topic1 = from = deployer (right-padded address)
    let expected_from = h.addrs.deployer.to_u256().to_be_bytes();
    assert_eq!(log.topics[1], expected_from);
    // topic2 = to = alice
    let expected_to = h.addrs.alice.to_u256().to_be_bytes();
    assert_eq!(log.topics[2], expected_to);
    // data = value = 77, ABI-encoded as 32-byte big-endian
    let mut expected_data = [0u8; 32];
    expected_data[31] = 77;
    assert_eq!(log.data.as_slice(), &expected_data);
}

#[test]
fn approve_then_allowance_roundtrips() {
    // Alice approves Bob for 555. The allowance view must then return the
    // same value for the (alice, bob) pair.
    let (mut h, c) = setup();
    let data = h.call_ok(
        c,
        h.addrs.alice,
        "approve(address,uint256)",
        &[h.addrs.bob.to_u256(), U256::from_u64(555)],
    );
    // Return value is `true` → 32-byte word with low byte 1.
    assert_eq!(data.last().copied(), Some(1u8));
    let v = h.view_u256(
        c,
        h.addrs.alice,
        "allowance(address,address)",
        &[h.addrs.alice.to_u256(), h.addrs.bob.to_u256()],
    );
    assert_eq!(v, U256::from_u64(555));
}

#[test]
fn approve_emits_event() {
    let (mut h, c) = setup();
    let _ = h.call_ok(
        c,
        h.addrs.alice,
        "approve(address,uint256)",
        &[h.addrs.bob.to_u256(), U256::from_u64(42)],
    );
    assert!(
        h.find_event("Approval(address,address,uint256)").is_some(),
        "no Approval event"
    );
}

#[test]
fn approval_event_has_canonical_topic_and_three_topics() {
    let (mut h, c) = setup();
    let _ = h.call_ok(
        c,
        h.addrs.alice,
        "approve(address,uint256)",
        &[h.addrs.bob.to_u256(), U256::from_u64(500)],
    );
    let log = h
        .find_event("Approval(address,address,uint256)")
        .expect("Approval event emitted");
    assert_eq!(
        log.topics[0],
        event_topic("Approval(address,address,uint256)")
    );
    assert_eq!(log.topics.len(), 3, "Approval must use LOG3");
    let expected_owner = h.addrs.alice.to_u256().to_be_bytes();
    assert_eq!(log.topics[1], expected_owner);
    let expected_spender = h.addrs.bob.to_u256().to_be_bytes();
    assert_eq!(log.topics[2], expected_spender);
}

#[test]
fn transferfrom_without_allowance_reverts() {
    let (mut h, c) = setup();
    // Bob has never been approved, so transferFrom must revert.
    let _data = h.call_revert(
        c,
        h.addrs.bob,
        "transferFrom(address,address,uint256)",
        &[
            h.addrs.alice.to_u256(),
            h.addrs.carol.to_u256(),
            U256::from_u64(10),
        ],
    );
}

// ---------------------------------------------------------------------------
// Dispatcher / selector guards
// ---------------------------------------------------------------------------

#[test]
fn unknown_selector_reverts() {
    let (mut h, c) = setup();
    // 0xdeadbeef is not mapped to any synthesized function.
    let input = vec![0xde, 0xad, 0xbe, 0xef];
    let result = h.raw_call(c, h.addrs.alice, input, U256::ZERO, false);
    assert!(result.is_revert(), "expected revert, got {result:?}");
}

#[test]
fn selectors_are_canonical_erc20() {
    use covenant_testing::abi::selector;
    let (h, c) = setup();
    let sels = &h.artifact(c).function_selectors;
    for (name, sig) in [
        ("totalSupply", "totalSupply()"),
        ("balanceOf", "balanceOf(address)"),
        ("transfer", "transfer(address,uint256)"),
        ("transferFrom", "transferFrom(address,address,uint256)"),
        ("approve", "approve(address,uint256)"),
        ("allowance", "allowance(address,address)"),
        ("decimals", "decimals()"),
        ("symbol", "symbol()"),
        ("name", "name()"),
    ] {
        assert_eq!(
            sels.get(name).copied(),
            Some(selector(sig)),
            "selector mismatch for {name}"
        );
    }
}

#[test]
fn two_approvals_produce_two_events() {
    let (mut h, c) = setup();
    let _ = h.call_ok(
        c,
        h.addrs.alice,
        "approve(address,uint256)",
        &[h.addrs.bob.to_u256(), U256::from_u64(1)],
    );
    let _ = h.call_ok(
        c,
        h.addrs.carol,
        "approve(address,uint256)",
        &[h.addrs.dave.to_u256(), U256::from_u64(2)],
    );
    assert_eq!(h.count_events("Approval(address,address,uint256)"), 2);
}

#[test]
fn symbol_returns_coin() {
    let (mut h, c) = setup();
    let s = h.view_string(c, h.addrs.alice, "symbol()", &[]);
    assert_eq!(s, "COIN");
}

#[test]
fn name_returns_covenant_coin() {
    let (mut h, c) = setup();
    let s = h.view_string(c, h.addrs.alice, "name()", &[]);
    assert_eq!(s, "Covenant Coin");
}
