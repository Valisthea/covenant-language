// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.20;

/**
 * ⚠ V0.9 PLACEHOLDER: NOT FOR PRODUCTION SECRETS ⚠
 *
 * This contract implements the Covenant V0.9 FHE helper interface
 * with MOCKED logic. It is suitable ONLY for:
 *   - Playground demos and developer onboarding
 *   - Sepolia / testnet integration testing
 *   - Audit dry-runs of the Covenant compiler routing layer
 *
 * It is NOT suitable for:
 *   - Storing real secrets (plaintexts are recoverable from chain state)
 *   - Verifying real signatures (no relevance: this is FHE)
 *   - Verifying real ZK proofs (no relevance: this is FHE)
 *
 * Production-grade implementations land in V1.0 after external audit.
 * Until then, this contract reverts on Ethereum mainnet (chainid 1).
 *
 * For the V0.9 → V1.0 swap-in plan, see:
 *   covenant-src/docs/v0.9/helper-source-audit-checklist.md
 */
contract MockedFHEHelper {
    mapping(bytes32 => uint256) private _handles;
    mapping(bytes32 => bool)    private _exists;
    uint256 private _nonce;

    event HandleMinted(bytes32 indexed handle, address indexed creator, bytes4 op);
    event MockedHelperUsed(bytes4 indexed selector, address indexed caller);

    error InvalidHandle();
    error MainnetForbidden();

    modifier notMainnet() {
        if (block.chainid == 1) revert MainnetForbidden();
        _;
    }

    function encryptTrivial(uint256 plaintext)
        external notMainnet returns (bytes32 handle)
    {
        unchecked { _nonce++; }
        handle = keccak256(abi.encode("covenant-fhe-trivial", _nonce, msg.sender));
        _handles[handle] = plaintext;
        _exists[handle] = true;
        emit HandleMinted(handle, msg.sender, this.encryptTrivial.selector);
        emit MockedHelperUsed(this.encryptTrivial.selector, msg.sender);
    }

    function encryptFresh(uint256 plaintext, uint256 randomNonce)
        external notMainnet returns (bytes32 handle)
    {
        handle = keccak256(abi.encode("covenant-fhe-fresh", randomNonce, msg.sender));
        _handles[handle] = plaintext;
        _exists[handle] = true;
        emit HandleMinted(handle, msg.sender, this.encryptFresh.selector);
        emit MockedHelperUsed(this.encryptFresh.selector, msg.sender);
    }

    function add(bytes32 a, bytes32 b)
        external notMainnet returns (bytes32 result)
    {
        if (!_exists[a] || !_exists[b]) revert InvalidHandle();
        unchecked { _nonce++; }
        result = keccak256(abi.encode("covenant-fhe-add", _nonce));
        unchecked {
            _handles[result] = _handles[a] + _handles[b];
        }
        _exists[result] = true;
        emit HandleMinted(result, msg.sender, this.add.selector);
        emit MockedHelperUsed(this.add.selector, msg.sender);
    }

    function sub(bytes32 a, bytes32 b)
        external notMainnet returns (bytes32 result)
    {
        if (!_exists[a] || !_exists[b]) revert InvalidHandle();
        unchecked { _nonce++; }
        result = keccak256(abi.encode("covenant-fhe-sub", _nonce));
        unchecked {
            _handles[result] = _handles[a] - _handles[b];
        }
        _exists[result] = true;
        emit HandleMinted(result, msg.sender, this.sub.selector);
        emit MockedHelperUsed(this.sub.selector, msg.sender);
    }

    function mul(bytes32 a, bytes32 b)
        external notMainnet returns (bytes32 result)
    {
        if (!_exists[a] || !_exists[b]) revert InvalidHandle();
        unchecked { _nonce++; }
        result = keccak256(abi.encode("covenant-fhe-mul", _nonce));
        unchecked {
            _handles[result] = _handles[a] * _handles[b];
        }
        _exists[result] = true;
        emit HandleMinted(result, msg.sender, this.mul.selector);
        emit MockedHelperUsed(this.mul.selector, msg.sender);
    }

    function eq(bytes32 a, bytes32 b)
        external notMainnet returns (bytes32 result)
    {
        if (!_exists[a] || !_exists[b]) revert InvalidHandle();
        unchecked { _nonce++; }
        result = keccak256(abi.encode("covenant-fhe-eq", _nonce));
        _handles[result] = _handles[a] == _handles[b] ? 1 : 0;
        _exists[result] = true;
        emit HandleMinted(result, msg.sender, this.eq.selector);
        emit MockedHelperUsed(this.eq.selector, msg.sender);
    }

    function lt(bytes32 a, bytes32 b)
        external notMainnet returns (bytes32 result)
    {
        if (!_exists[a] || !_exists[b]) revert InvalidHandle();
        unchecked { _nonce++; }
        result = keccak256(abi.encode("covenant-fhe-lt", _nonce));
        _handles[result] = _handles[a] < _handles[b] ? 1 : 0;
        _exists[result] = true;
        emit HandleMinted(result, msg.sender, this.lt.selector);
        emit MockedHelperUsed(this.lt.selector, msg.sender);
    }

    function cmux(bytes32 cond, bytes32 ifTrue, bytes32 ifFalse)
        external notMainnet returns (bytes32 result)
    {
        if (!_exists[cond] || !_exists[ifTrue] || !_exists[ifFalse]) {
            revert InvalidHandle();
        }
        unchecked { _nonce++; }
        result = keccak256(abi.encode("covenant-fhe-cmux", _nonce));
        _handles[result] = _handles[cond] != 0 ? _handles[ifTrue] : _handles[ifFalse];
        _exists[result] = true;
        emit HandleMinted(result, msg.sender, this.cmux.selector);
        emit MockedHelperUsed(this.cmux.selector, msg.sender);
    }

    /// @dev V0.9: open access. Real `reveal X to Y` access control is
    ///      enforced at compile time by the Covenant compiler, not here.
    ///      The `requester` parameter is part of the ABI for V1.0 forward
    ///      compatibility.
    function decrypt(bytes32 handle, address /* requester */)
        external view notMainnet returns (uint256)
    {
        if (!_exists[handle]) revert InvalidHandle();
        return _handles[handle];
    }
}
