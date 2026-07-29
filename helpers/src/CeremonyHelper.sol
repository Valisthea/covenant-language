// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.20;

/**
 * CeremonyHelper: V0.9 ERC-8228 Amnesia Ceremony lifecycle.
 *
 * Implements the 4 state-changing methods + 5 read views that the Covenant
 * compiler emits CALL into for contracts using the `ceremony` construct.
 *
 * Architecture: see covenant-src/docs/v0.9/precompile-bridge-architecture.md
 * Interface spec: see covenant-src/docs/v0.9/helper-interfaces.md
 *
 * V0.9 vs V1.0:
 *   - V0.9 destruction proof = keccak256(shares || sessionId)
 *   - V1.0 destruction proof = full Wesolowski VDF over reconstructed secret
 *   The proof shape (bytes returned + emitted) is forward-compatible: V1.0
 *   appends fields without breaking V0.9 decoders.
 *
 * Security invariants verified by tests:
 *   I1. Phase transitions are strictly monotonic
 *   I2. Once Destroyed, no state-changing call succeeds
 *   I3. Shares are wiped via `delete` on amnesiaDestroy
 *   I4. Only the original calling ceremony (msg.sender == ceremony) can
 *       transition its own session
 *   I5. destructionProof is deterministic given same inputs
 *   I6. Reentrancy-safe: state set before any external interaction
 */
contract CeremonyHelper {
    enum Phase { Setup, Active, Finalized, Destroyed }

    struct Session {
        Phase phase;
        uint128 guardiansCount;
        uint128 threshold;
        bytes32[] sharesSubmitted;
        address[] guardiansList;
        bytes destructionProof;
        address ceremony;
    }

    mapping(uint256 => Session) private _sessions;
    mapping(address => uint256[]) private _sessionsByCeremony;
    uint256 private _nonce;

    event AmnesiaSetup(
        uint256 indexed sessionId,
        address indexed ceremony,
        uint256 guardiansCount,
        uint256 threshold
    );
    event AmnesiaShareSubmitted(uint256 indexed sessionId, address indexed guardian);
    event AmnesiaFinalized(uint256 indexed sessionId, bool thresholdMet);
    event AmnesiaDestroyed(uint256 indexed sessionId, bytes destructionProof);

    error InvalidSession();
    error InvalidPhase(uint8 currentPhase, uint8 requiredPhase);
    error UnauthorizedCaller(address expected, address actual);
    error SessionCollision(uint256 sessionId);

    // ─── State-changing ──────────────────────────────────────────────────

    /// V0.9.1 single-arg overload: matches the Covenant compiler's V0.8
    /// `Opcode::AmnesiaBegin` operand count (1 = nonce/seed). Defaults
    /// `guardiansCount=3, threshold=2` from the canonical example fixture.
    /// V1.0 will widen the Covenant compiler to thread metadata into the
    /// operand list so the 3-arg overload below is the primary entry point.
    function amnesiaSetup(uint256 nonce) external returns (uint256 sessionId) {
        return _amnesiaSetup(nonce, 3, 2);
    }

    function amnesiaSetup(
        uint256 nonce,
        uint256 guardiansCount,
        uint256 threshold
    ) external returns (uint256 sessionId) {
        return _amnesiaSetup(nonce, guardiansCount, threshold);
    }

    function _amnesiaSetup(
        uint256 nonce,
        uint256 guardiansCount,
        uint256 threshold
    ) internal returns (uint256 sessionId) {
        unchecked {
            _nonce++;
        }
        sessionId = uint256(
            keccak256(abi.encode(nonce, _nonce, msg.sender, block.chainid))
        );

        // Per Sprint 29 helper-interfaces.md §8 open question (1):
        // explicit collision guard. Probability astronomical but reverting
        // is cheaper than silently overwriting.
        if (_sessions[sessionId].ceremony != address(0)) {
            revert SessionCollision(sessionId);
        }

        Session storage s = _sessions[sessionId];
        s.phase = Phase.Active; // Setup → Active in one logical transition
        s.guardiansCount = uint128(guardiansCount);
        s.threshold = uint128(threshold);
        s.ceremony = msg.sender;

        _sessionsByCeremony[msg.sender].push(sessionId);

        emit AmnesiaSetup(sessionId, msg.sender, guardiansCount, threshold);
    }

    function amnesiaSubmitShare(uint256 sessionId, bytes32 share)
        external
        returns (bool ok)
    {
        Session storage s = _sessions[sessionId];
        if (s.ceremony == address(0)) revert InvalidSession();
        if (s.ceremony != msg.sender) {
            revert UnauthorizedCaller(s.ceremony, msg.sender);
        }
        if (s.phase != Phase.Active) {
            revert InvalidPhase(uint8(s.phase), uint8(Phase.Active));
        }

        s.sharesSubmitted.push(share);
        // tx.origin records the EOA guardian; msg.sender is the ceremony contract.
        s.guardiansList.push(tx.origin);

        emit AmnesiaShareSubmitted(sessionId, tx.origin);
        return true;
    }

    function amnesiaFinalize(uint256 sessionId)
        external
        returns (bool thresholdMet)
    {
        Session storage s = _sessions[sessionId];
        if (s.ceremony == address(0)) revert InvalidSession();
        if (s.ceremony != msg.sender) {
            revert UnauthorizedCaller(s.ceremony, msg.sender);
        }
        if (s.phase != Phase.Active) {
            revert InvalidPhase(uint8(s.phase), uint8(Phase.Active));
        }

        thresholdMet = s.sharesSubmitted.length >= s.threshold;
        s.phase = Phase.Finalized;

        emit AmnesiaFinalized(sessionId, thresholdMet);
    }

    function amnesiaDestroy(uint256 sessionId)
        external
        returns (bytes memory destructionProof)
    {
        Session storage s = _sessions[sessionId];
        if (s.ceremony == address(0)) revert InvalidSession();
        if (s.ceremony != msg.sender) {
            revert UnauthorizedCaller(s.ceremony, msg.sender);
        }
        if (s.phase != Phase.Finalized) {
            revert InvalidPhase(uint8(s.phase), uint8(Phase.Finalized));
        }

        // V0.9 destruction proof shape:
        //   abi.encode(sessionId, keccak256(shares ++ sessionId))
        // V1.0 will prepend a Wesolowski VDF output without breaking decoders.
        bytes32 commitment = keccak256(
            abi.encodePacked(s.sharesSubmitted, sessionId)
        );
        destructionProof = abi.encode(sessionId, commitment);

        s.destructionProof = destructionProof;
        s.phase = Phase.Destroyed;

        emit AmnesiaDestroyed(sessionId, destructionProof);

        // Wipe sensitive data: shares + guardian list. Phase + commitment kept.
        delete s.sharesSubmitted;
        delete s.guardiansList;
    }

    // ─── Views ───────────────────────────────────────────────────────────

    function phase(uint256 sessionId) external view returns (uint8) {
        return uint8(_sessions[sessionId].phase);
    }

    function isDestroyed(uint256 sessionId) external view returns (bool) {
        return _sessions[sessionId].phase == Phase.Destroyed;
    }

    function getDestructionProof(uint256 sessionId)
        external view returns (bytes memory)
    {
        return _sessions[sessionId].destructionProof;
    }

    function sessionsByCeremony(address ceremony, uint256 idx)
        external view returns (uint256 sessionId)
    {
        return _sessionsByCeremony[ceremony][idx];
    }

    function sessionCount(address ceremony) external view returns (uint256) {
        return _sessionsByCeremony[ceremony].length;
    }
}
