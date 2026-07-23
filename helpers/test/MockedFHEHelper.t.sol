// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import "../src/MockedFHEHelper.sol";

contract MockedFHEHelperTest is Test {
    MockedFHEHelper internal helper;

    function setUp() public {
        helper = new MockedFHEHelper();
    }

    function test_EncryptAndDecryptRoundtrip() public {
        bytes32 h = helper.encryptTrivial(42);
        assertEq(helper.decrypt(h, address(this)), 42);
    }

    function test_HomomorphicAdd() public {
        bytes32 a = helper.encryptTrivial(10);
        bytes32 b = helper.encryptTrivial(32);
        bytes32 c = helper.add(a, b);
        assertEq(helper.decrypt(c, address(this)), 42);
    }

    function test_HomomorphicSub() public {
        bytes32 a = helper.encryptTrivial(100);
        bytes32 b = helper.encryptTrivial(58);
        bytes32 c = helper.sub(a, b);
        assertEq(helper.decrypt(c, address(this)), 42);
    }

    function test_HomomorphicMul() public {
        bytes32 a = helper.encryptTrivial(6);
        bytes32 b = helper.encryptTrivial(7);
        bytes32 c = helper.mul(a, b);
        assertEq(helper.decrypt(c, address(this)), 42);
    }

    function test_EqLt() public {
        bytes32 a = helper.encryptTrivial(5);
        bytes32 b = helper.encryptTrivial(5);
        bytes32 c = helper.encryptTrivial(7);
        bytes32 r1 = helper.eq(a, b); // 5 == 5 → 1
        bytes32 r2 = helper.eq(a, c); // 5 == 7 → 0
        bytes32 r3 = helper.lt(a, c); // 5 < 7  → 1
        bytes32 r4 = helper.lt(c, a); // 7 < 5  → 0
        assertEq(helper.decrypt(r1, address(this)), 1);
        assertEq(helper.decrypt(r2, address(this)), 0);
        assertEq(helper.decrypt(r3, address(this)), 1);
        assertEq(helper.decrypt(r4, address(this)), 0);
    }

    function test_Cmux() public {
        bytes32 t = helper.encryptTrivial(1);
        bytes32 f = helper.encryptTrivial(0);
        bytes32 a = helper.encryptTrivial(100);
        bytes32 b = helper.encryptTrivial(200);
        assertEq(helper.decrypt(helper.cmux(t, a, b), address(this)), 100);
        assertEq(helper.decrypt(helper.cmux(f, a, b), address(this)), 200);
    }

    function test_RevertOnInvalidHandle() public {
        vm.expectRevert(MockedFHEHelper.InvalidHandle.selector);
        helper.add(bytes32(uint256(0xdead)), bytes32(uint256(0xbeef)));
    }

    function test_RevertOnInvalidHandleDecrypt() public {
        vm.expectRevert(MockedFHEHelper.InvalidHandle.selector);
        helper.decrypt(bytes32(uint256(0xdead)), address(this));
    }

    function test_MainnetForbidden() public {
        vm.chainId(1);
        vm.expectRevert(MockedFHEHelper.MainnetForbidden.selector);
        helper.encryptTrivial(1);
    }

    function test_TestnetAllowed() public {
        vm.chainId(11155111); // Sepolia
        bytes32 h = helper.encryptTrivial(7);
        assertEq(helper.decrypt(h, address(this)), 7);
    }

    function test_HandlesAreUnique() public {
        bytes32 h1 = helper.encryptTrivial(42);
        bytes32 h2 = helper.encryptTrivial(42);
        assertTrue(h1 != h2, "trivial encryption should still produce unique handles via nonce");
    }
}
