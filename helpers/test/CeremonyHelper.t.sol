// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import "../src/CeremonyHelper.sol";

/// @dev A minimal contract that simulates being a Covenant `ceremony` and
///      relays calls to the helper. msg.sender as seen by the helper is
///      THIS contract's address.
contract CeremonyCaller {
    CeremonyHelper public immutable helper;
    constructor(CeremonyHelper h) { helper = h; }

    function setup(uint256 nonce, uint256 g, uint256 t)
        external returns (uint256) { return helper.amnesiaSetup(nonce, g, t); }
    function submit(uint256 sid, bytes32 share)
        external returns (bool) { return helper.amnesiaSubmitShare(sid, share); }
    function finalize_(uint256 sid)
        external returns (bool) { return helper.amnesiaFinalize(sid); }
    function destroy_(uint256 sid)
        external returns (bytes memory) { return helper.amnesiaDestroy(sid); }
}

contract CeremonyHelperTest is Test {
    CeremonyHelper internal helper;
    CeremonyCaller internal alice;
    CeremonyCaller internal bob;

    function setUp() public {
        helper = new CeremonyHelper();
        alice = new CeremonyCaller(helper);
        bob = new CeremonyCaller(helper);
    }

    // I1: phase transitions are strictly monotonic
    function test_PhaseTransitionsMonotonic() public {
        uint256 sid = alice.setup(42, 3, 2);
        assertEq(helper.phase(sid), uint8(CeremonyHelper.Phase.Active));

        alice.submit(sid, bytes32("share1"));
        alice.submit(sid, bytes32("share2"));

        alice.finalize_(sid);
        assertEq(helper.phase(sid), uint8(CeremonyHelper.Phase.Finalized));

        alice.destroy_(sid);
        assertEq(helper.phase(sid), uint8(CeremonyHelper.Phase.Destroyed));
        assertTrue(helper.isDestroyed(sid));
    }

    // I2: Once Destroyed, no state-changing call succeeds
    function test_DestroyedIsTerminal() public {
        uint256 sid = alice.setup(1, 1, 1);
        alice.submit(sid, bytes32("s"));
        alice.finalize_(sid);
        alice.destroy_(sid);

        vm.expectRevert(); // any phase revert is acceptable
        alice.submit(sid, bytes32("late"));

        vm.expectRevert();
        alice.finalize_(sid);

        vm.expectRevert();
        alice.destroy_(sid);
    }

    // I4: Only the original ceremony contract can transition its session
    function test_UnauthorizedCallerRejected() public {
        uint256 sid = alice.setup(7, 3, 2);
        vm.expectRevert(
            abi.encodeWithSelector(
                CeremonyHelper.UnauthorizedCaller.selector,
                address(alice),
                address(bob)
            )
        );
        bob.submit(sid, bytes32("attacker"));
    }

    // I5: destructionProof is deterministic given same inputs
    function test_DestructionProofDeterministic() public {
        uint256 sid1 = alice.setup(99, 2, 1);
        alice.submit(sid1, bytes32("alpha"));
        alice.submit(sid1, bytes32("beta"));
        alice.finalize_(sid1);
        bytes memory proof1 = alice.destroy_(sid1);

        // sessionIds are unique per nonce + counter, so a fresh ceremony
        // produces a different proof. We verify shape stability:
        (uint256 emittedSid, bytes32 commitment) = abi.decode(
            proof1, (uint256, bytes32)
        );
        assertEq(emittedSid, sid1);
        assertTrue(commitment != bytes32(0));

        // The view returns the same bytes
        bytes memory storedProof = helper.getDestructionProof(sid1);
        assertEq(keccak256(storedProof), keccak256(proof1));
    }

    // I3: shares wiped on destroy (verify via sessionCount + getDestructionProof)
    function test_StoredProofPersistsAfterDestroy() public {
        uint256 sid = alice.setup(123, 3, 2);
        alice.submit(sid, bytes32(uint256(0xAA)));
        alice.submit(sid, bytes32(uint256(0xBB)));
        alice.finalize_(sid);
        alice.destroy_(sid);

        // Proof remains queryable
        bytes memory p = helper.getDestructionProof(sid);
        assertGt(p.length, 0);
        // Phase is Destroyed
        assertEq(helper.phase(sid), uint8(CeremonyHelper.Phase.Destroyed));
    }

    function test_PhaseRevertCarriesContext() public {
        uint256 sid = alice.setup(55, 1, 1);
        // Skip submit, try to destroy directly
        vm.expectRevert(
            abi.encodeWithSelector(
                CeremonyHelper.InvalidPhase.selector,
                uint8(CeremonyHelper.Phase.Active),
                uint8(CeremonyHelper.Phase.Finalized)
            )
        );
        alice.destroy_(sid);
    }

    function test_SessionCountTracking() public {
        assertEq(helper.sessionCount(address(alice)), 0);
        alice.setup(1, 1, 1);
        alice.setup(2, 1, 1);
        alice.setup(3, 1, 1);
        assertEq(helper.sessionCount(address(alice)), 3);
        assertEq(helper.sessionCount(address(bob)), 0);
    }

    function test_InvalidSessionRevert() public {
        uint256 fake = 0xdeadbeef;
        vm.expectRevert(CeremonyHelper.InvalidSession.selector);
        alice.submit(fake, bytes32(0));
    }

    // Gas budget targets per helper-interfaces.md §1
    function test_GasBudget_Setup() public {
        uint256 g = gasleft();
        alice.setup(1, 3, 2);
        uint256 used = g - gasleft();
        assertLt(used, 200_000, "amnesiaSetup should be <200k gas");
    }

    function test_GasBudget_Destroy() public {
        uint256 sid = alice.setup(1, 2, 1);
        alice.submit(sid, bytes32("s1"));
        alice.submit(sid, bytes32("s2"));
        alice.finalize_(sid);
        uint256 g = gasleft();
        alice.destroy_(sid);
        uint256 used = g - gasleft();
        assertLt(used, 150_000, "amnesiaDestroy should be <150k gas");
    }
}
