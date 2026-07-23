//! End-to-end regression coverage for `list<Struct>` support (OMEGA V6,
//! 2026-07-05 cycle: CRT-003 follow-on + MED-001 follow-on).
//!
//! Before this cycle, `append` built a struct value via `StructNew` and then
//! discarded it (nothing was ever written to storage); `for each` executed
//! its body exactly once regardless of collection length, with the loop
//! variable never bound to a real element; and `list[idx].field = value`
//! was a hard no-op at the IR level (on top of the privacy analyzer
//! separately, incorrectly rejecting the same syntax as an E301 privacy
//! violation whenever the field was `ciphertext<T>`). This test exercises
//! the full lifecycle -- append, iterate, read, and write -- against the
//! real compiler and the real mini-EVM interpreter.

use covenant_testing::{CovenantTestHarness, U256};

const SRC: &str = r#"
record BallotBox {
    struct Vote {
        voter: address
        weight: amount
    }

    votes: [Vote] = []
    total: amount = 0

    action cast(w: amount) {
        append votes { voter: caller, weight: w }
    }

    action tally() {
        total = 0
        for each v in votes {
            total = total + v.weight
        }
    }

    action reweigh(idx: amount, w: amount) {
        votes[idx].weight = w
    }

    action peek(idx: amount) {
        total = votes[idx].weight
    }

    view get_total returns amount { total }
    view get_count returns amount { votes.length }
}
"#;

#[test]
fn append_foreach_and_struct_field_write_work_end_to_end() {
    let mut h = CovenantTestHarness::new();
    let c = h.deploy(SRC, h.addrs.deployer).expect("deploy");
    let alice = h.addrs.alice;

    // Nothing appended yet.
    assert_eq!(h.view_u256(c, alice, "get_count()", &[]), U256::ZERO);

    h.call_ok(c, alice, "cast(uint256)", &[U256::from_u64(10)]);
    h.call_ok(c, alice, "cast(uint256)", &[U256::from_u64(20)]);
    h.call_ok(c, alice, "cast(uint256)", &[U256::from_u64(30)]);

    // `append` actually persists elements.
    assert_eq!(h.view_u256(c, alice, "get_count()", &[]), U256::from_u64(3));

    h.call_ok(c, alice, "tally()", &[]);
    // `for each` actually iterates every element (not just the first).
    assert_eq!(
        h.view_u256(c, alice, "get_total()", &[]),
        U256::from_u64(60)
    );

    // Reading `list[idx].field` as an expression resolves the correct
    // element AND the correct field index.
    h.call_ok(c, alice, "peek(uint256)", &[U256::ZERO]);
    assert_eq!(
        h.view_u256(c, alice, "get_total()", &[]),
        U256::from_u64(10)
    );
    h.call_ok(c, alice, "peek(uint256)", &[U256::from_u64(1)]);
    assert_eq!(
        h.view_u256(c, alice, "get_total()", &[]),
        U256::from_u64(20)
    );
    h.call_ok(c, alice, "peek(uint256)", &[U256::from_u64(2)]);
    assert_eq!(
        h.view_u256(c, alice, "get_total()", &[]),
        U256::from_u64(30)
    );

    // Writing `list[idx].field = value` actually persists.
    h.call_ok(
        c,
        alice,
        "reweigh(uint256,uint256)",
        &[U256::ZERO, U256::from_u64(100)],
    );
    h.call_ok(c, alice, "peek(uint256)", &[U256::ZERO]);
    assert_eq!(
        h.view_u256(c, alice, "get_total()", &[]),
        U256::from_u64(100)
    );

    // The write is visible to a subsequent `for each` too, and other
    // elements are undisturbed.
    h.call_ok(c, alice, "tally()", &[]);
    assert_eq!(
        h.view_u256(c, alice, "get_total()", &[]),
        U256::from_u64(150)
    );
}
