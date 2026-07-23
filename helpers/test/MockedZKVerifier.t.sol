// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import "../src/MockedZKVerifier.sol";

contract MockedZKVerifierTest is Test {
    MockedZKVerifier internal helper;

    function setUp() public {
        helper = new MockedZKVerifier();
    }

    function test_VerifyShortProofRejected() public {
        bytes memory shortProof = new bytes(100);
        vm.expectRevert(
            abi.encodeWithSelector(MockedZKVerifier.InvalidProofFormat.selector, 100)
        );
        helper.verify(bytes32("vk"), bytes(""), shortProof);
    }

    function test_VerifyAcceptsLongEnoughProof() public view {
        bytes memory proof = new bytes(256);
        bool _ok = helper.verify(bytes32("vk"), bytes("inputs"), proof);
        _ok;
    }

    function test_NullifierDeterministicAndCorrect() public view {
        bytes32 secret = bytes32(uint256(0xCAFEBABE));
        bytes32 n1 = helper.nullifier(secret);
        bytes32 n2 = helper.nullifier(secret);
        assertEq(n1, n2, "nullifier(secret) deterministic");
        // Different secret → different nullifier
        bytes32 n3 = helper.nullifier(bytes32(uint256(0x1234)));
        assertTrue(n1 != n3);
        // Sanity: matches expected format keccak256("nullifier" || secret)
        bytes32 expected = keccak256(abi.encodePacked("nullifier", secret));
        assertEq(n1, expected);
    }

    function test_AggregateReturnsStub() public view {
        bytes memory proofs = abi.encode(uint256(1), uint256(2), uint256(3));
        bytes memory agg = helper.proofAggregate(proofs);
        assertGt(agg.length, 16, "aggregate has tag prefix + commitment");
    }

    function test_MainnetForbiddenForVerify() public {
        vm.chainId(1);
        bytes memory proof = new bytes(256);
        vm.expectRevert(MockedZKVerifier.MainnetForbidden.selector);
        helper.verify(bytes32("vk"), bytes(""), proof);
    }

    function test_TestnetAllowedForVerify() public {
        vm.chainId(11155111);
        bytes memory proof = new bytes(256);
        helper.verify(bytes32("vk"), bytes(""), proof); // no revert
    }
}
