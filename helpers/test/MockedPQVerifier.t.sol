// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import "../src/MockedPQVerifier.sol";

contract MockedPQVerifierTest is Test {
    MockedPQVerifier internal helper;

    function setUp() public {
        helper = new MockedPQVerifier();
    }

    function test_LengthCheckSig() public {
        bytes memory badSig = new bytes(100);
        bytes memory pk = new bytes(2592);
        vm.expectRevert(
            abi.encodeWithSelector(
                MockedPQVerifier.InvalidSignatureLength.selector,
                100,
                4595
            )
        );
        helper.pqVerify(bytes32("msg"), badSig, pk);
    }

    function test_LengthCheckPubKey() public {
        bytes memory sig = new bytes(4595);
        bytes memory badPk = new bytes(100);
        vm.expectRevert(
            abi.encodeWithSelector(
                MockedPQVerifier.InvalidPublicKeyLength.selector,
                100,
                2592
            )
        );
        helper.pqVerify(bytes32("msg"), sig, badPk);
    }

    function test_VerifyReturnsBool() public view {
        bytes memory sig = new bytes(4595);
        bytes memory pk  = new bytes(2592);
        // Just verify it returns without revert when lengths are correct.
        // The bool return is meaningless in V0.9 (parity check).
        bool _ok = helper.pqVerify(bytes32("msg"), sig, pk);
        // Doesn't matter if true or false — V0.9 banner explicitly disclaims this.
        _ok; // silence unused
    }

    function test_Keygen() public view {
        bytes memory pk = helper.pqKeygenFromSeed(0xC0FFEE);
        assertEq(pk.length, 2592);
        // Same seed → same key (deterministic)
        bytes memory pk2 = helper.pqKeygenFromSeed(0xC0FFEE);
        assertEq(keccak256(pk), keccak256(pk2));
        // Different seed → different key
        bytes memory pk3 = helper.pqKeygenFromSeed(0xDEAD);
        assertTrue(keccak256(pk) != keccak256(pk3));
    }

    function test_RandomDifferentEachCall() public {
        bytes32 r1 = helper.pqRandom(1);
        vm.warp(block.timestamp + 1);
        bytes32 r2 = helper.pqRandom(1);
        assertTrue(r1 != r2, "timestamp delta should change output");
    }

    function test_MainnetForbiddenForVerify() public {
        // pqVerify is `view`; the modifier still applies on EVM CALL.
        // notMainnet only fires on actual chainid 1.
        vm.chainId(1);
        bytes memory sig = new bytes(4595);
        bytes memory pk  = new bytes(2592);
        vm.expectRevert(MockedPQVerifier.MainnetForbidden.selector);
        helper.pqVerify(bytes32("msg"), sig, pk);
    }

    function test_TestnetAllowed() public {
        vm.chainId(11155111);
        bytes memory sig = new bytes(4595);
        bytes memory pk  = new bytes(2592);
        helper.pqVerify(bytes32("msg"), sig, pk); // no revert
    }
}
