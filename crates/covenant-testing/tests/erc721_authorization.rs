//! End-to-end ERC-721 authorization / lifecycle tests (V0.9.2 hardening).
//!
//! Regression coverage for DEBT.md "ERC-721 transferFrom permissive". Before
//! V0.9.2 the auto-synthesized `transferFrom` only checked `ownerOf == from`
//! and never the caller, so ANY account could move ANY token (NFT theft).
//! It also accepted `to == address(0)`. These tests deploy a bare `nft`
//! construct and exercise the runtime through the mini-EVM interpreter.
//!
//! Since V0.9.7 (F-16) the synthesized `mint` is gated on the deployer, so the
//! scenarios below are set up by having the deployer mint TO the account that
//! the test needs as owner. The recipient is still arbitrary, so every
//! authorization assertion is exercised against exactly the same ownership
//! layout as before: only the account that creates the supply changed.
//! `only_the_deployer_can_mint` pins that gate so a regression cannot quietly
//! turn these setups back into an open-access mint.

use covenant_testing::{CovenantTestHarness, U256};

const NFT_SRC: &str = r#"
nft AuditNFT {
    name: "Audit NFT"
    symbol: "ANFT"
    base_uri: "https://example.com/api/"
}
"#;

const MINT: &str = "mint(address,uint256)";
const OWNER_OF: &str = "ownerOf(uint256)";
const BALANCE_OF: &str = "balanceOf(address)";
const TRANSFER_FROM: &str = "transferFrom(address,address,uint256)";
const APPROVE: &str = "approve(address,uint256)";
const SET_APPROVAL_FOR_ALL: &str = "setApprovalForAll(address,bool)";
const BURN: &str = "burn(uint256)";

fn one() -> U256 {
    U256::from_u64(1)
}

#[test]
fn unauthorized_caller_cannot_steal_token() {
    let mut h = CovenantTestHarness::new();
    let nft = h.deploy(NFT_SRC, h.addrs.deployer).expect("deploy");
    let deployer = h.addrs.deployer;
    let alice = h.addrs.alice;
    let mallory = h.addrs.carol;
    let id = one();

    // Token #1 is minted to Alice, so Alice is the owner.
    h.call_ok(nft, deployer, MINT, &[alice.to_u256(), id]);
    assert_eq!(h.view_u256(nft, alice, OWNER_OF, &[id]), alice.to_u256());

    // Mallory is neither owner, approved, nor operator: the transfer must revert
    // even though `from` (alice) genuinely owns the token.
    h.call_revert(
        nft,
        mallory,
        TRANSFER_FROM,
        &[alice.to_u256(), mallory.to_u256(), id],
    );

    // Ownership and balances are untouched.
    assert_eq!(h.view_u256(nft, alice, OWNER_OF, &[id]), alice.to_u256());
    assert_eq!(
        h.view_u256(nft, alice, BALANCE_OF, &[alice.to_u256()]),
        one()
    );
    assert_eq!(
        h.view_u256(nft, alice, BALANCE_OF, &[mallory.to_u256()]),
        U256::ZERO
    );
}

#[test]
fn owner_can_transfer() {
    let mut h = CovenantTestHarness::new();
    let nft = h.deploy(NFT_SRC, h.addrs.deployer).expect("deploy");
    let deployer = h.addrs.deployer;
    let alice = h.addrs.alice;
    let bob = h.addrs.bob;
    let id = one();

    h.call_ok(nft, deployer, MINT, &[alice.to_u256(), id]);
    h.call_ok(
        nft,
        alice,
        TRANSFER_FROM,
        &[alice.to_u256(), bob.to_u256(), id],
    );

    assert_eq!(h.view_u256(nft, alice, OWNER_OF, &[id]), bob.to_u256());
    assert_eq!(
        h.view_u256(nft, alice, BALANCE_OF, &[alice.to_u256()]),
        U256::ZERO
    );
    assert_eq!(h.view_u256(nft, alice, BALANCE_OF, &[bob.to_u256()]), one());
}

#[test]
fn per_token_approved_can_transfer() {
    let mut h = CovenantTestHarness::new();
    let nft = h.deploy(NFT_SRC, h.addrs.deployer).expect("deploy");
    let alice = h.addrs.alice;
    let bob = h.addrs.bob;
    let carol = h.addrs.carol;
    let deployer = h.addrs.deployer;
    let id = one();

    h.call_ok(nft, deployer, MINT, &[alice.to_u256(), id]);
    // Alice approves Carol for this single token.
    h.call_ok(nft, alice, APPROVE, &[carol.to_u256(), id]);
    // Carol may now move it on Alice's behalf.
    h.call_ok(
        nft,
        carol,
        TRANSFER_FROM,
        &[alice.to_u256(), bob.to_u256(), id],
    );

    assert_eq!(h.view_u256(nft, alice, OWNER_OF, &[id]), bob.to_u256());
}

#[test]
fn approved_operator_can_transfer() {
    let mut h = CovenantTestHarness::new();
    let nft = h.deploy(NFT_SRC, h.addrs.deployer).expect("deploy");
    let alice = h.addrs.alice;
    let bob = h.addrs.bob;
    let carol = h.addrs.carol;
    let deployer = h.addrs.deployer;
    let id = one();

    h.call_ok(nft, deployer, MINT, &[alice.to_u256(), id]);
    // Alice authorizes Carol as an operator for ALL her tokens.
    h.call_ok(
        nft,
        alice,
        SET_APPROVAL_FOR_ALL,
        &[carol.to_u256(), U256::from_u64(1)],
    );
    h.call_ok(
        nft,
        carol,
        TRANSFER_FROM,
        &[alice.to_u256(), bob.to_u256(), id],
    );

    assert_eq!(h.view_u256(nft, alice, OWNER_OF, &[id]), bob.to_u256());
}

#[test]
fn transfer_to_zero_address_reverts() {
    let mut h = CovenantTestHarness::new();
    let nft = h.deploy(NFT_SRC, h.addrs.deployer).expect("deploy");
    let deployer = h.addrs.deployer;
    let alice = h.addrs.alice;
    let id = one();

    h.call_ok(nft, deployer, MINT, &[alice.to_u256(), id]);
    // Even the owner cannot transfer to the zero address (ERC721InvalidReceiver).
    h.call_revert(
        nft,
        alice,
        TRANSFER_FROM,
        &[alice.to_u256(), U256::ZERO, id],
    );
    // Token still owned by Alice: no accidental burn.
    assert_eq!(h.view_u256(nft, alice, OWNER_OF, &[id]), alice.to_u256());
}

#[test]
fn transfer_from_wrong_owner_reverts() {
    let mut h = CovenantTestHarness::new();
    let nft = h.deploy(NFT_SRC, h.addrs.deployer).expect("deploy");
    let alice = h.addrs.alice;
    let bob = h.addrs.bob;
    let deployer = h.addrs.deployer;
    let id = one();

    h.call_ok(nft, deployer, MINT, &[alice.to_u256(), id]);
    // `from` (bob) is not the real owner, so this reverts NotTokenOwner.
    h.call_revert(
        nft,
        alice,
        TRANSFER_FROM,
        &[bob.to_u256(), bob.to_u256(), id],
    );
    assert_eq!(h.view_u256(nft, alice, OWNER_OF, &[id]), alice.to_u256());
}

#[test]
fn burn_requires_authorization_and_clears_ownership() {
    let mut h = CovenantTestHarness::new();
    let nft = h.deploy(NFT_SRC, h.addrs.deployer).expect("deploy");
    let alice = h.addrs.alice;
    let mallory = h.addrs.carol;
    let deployer = h.addrs.deployer;
    let id = one();

    h.call_ok(nft, deployer, MINT, &[alice.to_u256(), id]);

    // A stranger cannot burn Alice's token.
    h.call_revert(nft, mallory, BURN, &[id]);
    assert_eq!(h.view_u256(nft, alice, OWNER_OF, &[id]), alice.to_u256());

    // The owner can burn it: ownerOf resets to zero and the balance drops.
    h.call_ok(nft, alice, BURN, &[id]);
    assert_eq!(h.view_u256(nft, alice, OWNER_OF, &[id]), U256::ZERO);
    assert_eq!(
        h.view_u256(nft, alice, BALANCE_OF, &[alice.to_u256()]),
        U256::ZERO
    );
}

#[test]
fn burn_nonexistent_token_reverts() {
    let mut h = CovenantTestHarness::new();
    let nft = h.deploy(NFT_SRC, h.addrs.deployer).expect("deploy");
    let alice = h.addrs.alice;

    // Token #999 was never minted → TokenDoesNotExist.
    h.call_revert(nft, alice, BURN, &[U256::from_u64(999)]);
}

#[test]
fn only_the_deployer_can_mint() {
    // F-16: `mint` is not part of EIP-721 and creates supply out of nothing, so
    // before V0.9.7 any account could mint any unminted id to any address. The
    // other tests in this file now mint as the deployer to set up their state,
    // which would keep passing if the gate were removed. This one fails loudly
    // in that case: it is the end-to-end counterpart of the IR-shape check in
    // covenant-stdlib's `f16_synthesized_mint_is_gated_on_the_deployer`.
    let mut h = CovenantTestHarness::new();
    let nft = h.deploy(NFT_SRC, h.addrs.deployer).expect("deploy");
    let deployer = h.addrs.deployer;
    let alice = h.addrs.alice;
    let id = one();

    // Alice cannot mint, not even to herself.
    h.call_revert(nft, alice, MINT, &[alice.to_u256(), id]);
    assert_eq!(h.view_u256(nft, alice, OWNER_OF, &[id]), U256::ZERO);
    assert_eq!(
        h.view_u256(nft, alice, BALANCE_OF, &[alice.to_u256()]),
        U256::ZERO
    );

    // The deployer can, and the recipient is free.
    h.call_ok(nft, deployer, MINT, &[alice.to_u256(), id]);
    assert_eq!(h.view_u256(nft, alice, OWNER_OF, &[id]), alice.to_u256());
}

#[test]
fn transfer_from_zero_owner_cannot_free_mint() {
    // An unminted token has owners[id] == 0. With from=address(0) the owner
    // match (ownerOf(id)==from) passes, but the caller is not authorized over
    // the zero owner, so the authorization gate blocks the "free mint" steal.
    let mut h = CovenantTestHarness::new();
    let nft = h.deploy(NFT_SRC, h.addrs.deployer).expect("deploy");
    let attacker = h.addrs.carol;
    let id = U256::from_u64(7); // never minted

    h.call_revert(
        nft,
        attacker,
        TRANSFER_FROM,
        &[U256::ZERO, attacker.to_u256(), id],
    );
    // Token remains unminted.
    assert_eq!(h.view_u256(nft, attacker, OWNER_OF, &[id]), U256::ZERO);
}
