// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.20;

/**
 * ⚠ V0.9 PLACEHOLDER: NOT FOR PRODUCTION SECRETS ⚠
 *
 * This contract implements the Covenant V0.9 ZK helper interface
 * with MOCKED logic for `verify` and `proofAggregate`. The
 * `nullifier` function is real (deterministic keccak hash) and
 * suitable for double-spend prevention even in V0.9.
 *
 * It is NOT suitable for:
 *   - Verifying real Halo2 SNARKs (this is a parity check)
 *   - Aggregating real Nova IVC proofs (returns a stub blob)
 *
 * Production-grade verifier lands in V1.0 (~250-400k gas per verify).
 * Until then, this contract reverts on Ethereum mainnet (chainid 1).
 *
 * See covenant-src/docs/v0.9/helper-source-audit-checklist.md
 */
contract MockedZKVerifier {
    event MockedHelperUsed(bytes4 indexed selector, address indexed caller);

    error InvalidProofFormat(uint256 length);
    error MainnetForbidden();

    modifier notMainnet() {
        if (block.chainid == 1) revert MainnetForbidden();
        _;
    }

    /// @notice Verify a Halo2 SNARK proof.
    /// @dev V0.9 = parity check; V1.0 = real Halo2 verifier (~250-400k gas).
    function verify(
        bytes32 vk,
        bytes calldata publicInputs,
        bytes calldata proof
    ) external view notMainnet returns (bool ok) {
        // Length floor: a real Halo2 proof is at least ~256 bytes.
        if (proof.length < 256) revert InvalidProofFormat(proof.length);
        ok = uint256(keccak256(abi.encodePacked(vk, publicInputs, proof))) % 2 == 0;
    }

    /// @notice Derive a nullifier from a secret.
    /// @dev REAL implementation, not mocked: keccak256("nullifier" || secret).
    ///      Suitable for double-spend prevention even under V0.9.
    function nullifier(bytes32 secret) external pure returns (bytes32) {
        return keccak256(abi.encodePacked("nullifier", secret));
    }

    /// @notice Aggregate multiple proofs (Nova IVC fold).
    /// @dev V0.9 stub; V1.0 = real Nova IVC accumulator.
    function proofAggregate(bytes calldata proofs)
        external pure returns (bytes memory)
    {
        return abi.encodePacked(bytes16("AGG_V0_9_STUB"), keccak256(proofs));
    }
}
