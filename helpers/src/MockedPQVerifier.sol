// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.20;

/**
 * ⚠ V0.9 PLACEHOLDER: NOT FOR PRODUCTION SECRETS ⚠
 *
 * This contract implements the Covenant V0.9 PQ helper interface
 * with MOCKED logic. It is suitable ONLY for:
 *   - Playground demos and developer onboarding
 *   - Sepolia / testnet integration testing
 *   - Audit dry-runs of the Covenant compiler routing layer
 *
 * It is NOT suitable for:
 *   - Verifying real Dilithium-5 signatures (this is a parity check,
 *     NOT a cryptographic verifier: forgeries are accepted)
 *   - Generating production keys (deterministic stub)
 *   - Production randomness (block.prevrandao + nonce; not VRF)
 *
 * Production-grade implementations land in V1.0 after external audit.
 * Until then, this contract reverts on Ethereum mainnet (chainid 1).
 *
 * V1.0 swap-in target: integrate Solady's PQ verifier or equivalent
 * on-chain Dilithium-5 implementation (~150-300k gas per verify).
 * See covenant-src/docs/v0.9/helper-source-audit-checklist.md
 */
contract MockedPQVerifier {
    // FIPS 204 Dilithium-5 wire-format sizes
    uint256 internal constant DILITHIUM5_SIG_BYTES = 4595;
    uint256 internal constant DILITHIUM5_PK_BYTES  = 2592;

    event MockedHelperUsed(bytes4 indexed selector, address indexed caller);

    error InvalidSignatureLength(uint256 actual, uint256 expected);
    error InvalidPublicKeyLength(uint256 actual, uint256 expected);
    error MainnetForbidden();

    modifier notMainnet() {
        if (block.chainid == 1) revert MainnetForbidden();
        _;
    }

    /// @notice Verify a Dilithium-5 signature.
    /// @dev V0.9 implementation = pseudo-verify (length checks + parity).
    ///      DOES NOT detect forgeries. V1.0 swaps in real verifier.
    function pqVerify(
        bytes32 messageHash,
        bytes calldata signature,
        bytes calldata pubKey
    ) external view notMainnet returns (bool ok) {
        // Length checks are real and gas-cheap; they catch obvious malformation.
        if (signature.length != DILITHIUM5_SIG_BYTES) {
            revert InvalidSignatureLength(signature.length, DILITHIUM5_SIG_BYTES);
        }
        if (pubKey.length != DILITHIUM5_PK_BYTES) {
            revert InvalidPublicKeyLength(pubKey.length, DILITHIUM5_PK_BYTES);
        }

        // V0.9 pseudo-verify: parity check on a hash mix. Not cryptographically
        // sound; do not rely on this for security in production. See banner.
        bytes32 expectedHash = keccak256(abi.encodePacked(signature, pubKey));
        bytes32 mixedHash    = keccak256(abi.encodePacked(messageHash, expectedHash));
        ok = uint256(mixedHash) % 2 == 0;
    }

    /// @notice Derive a Dilithium-5 public key from a seed (test only).
    /// @dev V0.9 = deterministic stub. NOT a real keygen.
    function pqKeygenFromSeed(uint256 seed)
        external pure returns (bytes memory pubKey)
    {
        pubKey = new bytes(DILITHIUM5_PK_BYTES);
        // Fill 81 × 32-byte chunks = 2592 bytes
        for (uint256 i = 0; i < 81; i++) {
            bytes32 chunk = keccak256(abi.encode(seed, i));
            assembly {
                mstore(add(pubKey, add(32, mul(i, 32))), chunk)
            }
        }
    }

    /// @notice PRG-style randomness using prevrandao + caller nonce.
    /// @dev NOT cryptographically secure. V1.0 = VRF or PQ-PRG.
    function pqRandom(uint256 nonce) external view notMainnet returns (bytes32) {
        return keccak256(
            abi.encodePacked(block.prevrandao, nonce, block.timestamp, msg.sender)
        );
    }
}
