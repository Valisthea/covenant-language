// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.20;

import "forge-std/Script.sol";
import "../src/CeremonyHelper.sol";
import "../src/MockedFHEHelper.sol";
import "../src/MockedPQVerifier.sol";
import "../src/MockedZKVerifier.sol";

/**
 * Deploy script for the V0.9.0 Covenant helpers.
 *
 * CREATE2 with deterministic salts so addresses are reproducible across:
 *   - dry-run (forge script Deploy)
 *   - actual deploy (forge script Deploy --broadcast)
 *   - V0.9.x bug-fix re-deploys (same salt → same address as long as the
 *     init bytecode is byte-identical; if init bytecode changes, address
 *     changes and the JSON registry bumps to V0.9.x+1)
 *
 * Usage:
 *   forge script script/Deploy.s.sol \
 *     --rpc-url $SEPOLIA_RPC_URL \
 *     --private-key $DEPLOYER_PK \
 *     --broadcast \
 *     --verify
 */
contract Deploy is Script {
    bytes32 internal constant SALT_CEREMONY = keccak256("covenant-v0.9.1-ceremony");
    bytes32 internal constant SALT_FHE      = keccak256("covenant-v0.9.0-fhe");
    bytes32 internal constant SALT_PQ       = keccak256("covenant-v0.9.0-pq");
    bytes32 internal constant SALT_ZK       = keccak256("covenant-v0.9.0-zk");

    function run() external {
        vm.startBroadcast();

        CeremonyHelper ceremony = new CeremonyHelper{salt: SALT_CEREMONY}();
        MockedFHEHelper fhe     = new MockedFHEHelper{salt: SALT_FHE}();
        MockedPQVerifier pq     = new MockedPQVerifier{salt: SALT_PQ}();
        MockedZKVerifier zk     = new MockedZKVerifier{salt: SALT_ZK}();

        vm.stopBroadcast();

        // Print machine-grep-able output for the address-capture script
        console.log("===HELPERS_DEPLOYED===");
        console.log("CHAIN_ID:",           block.chainid);
        console.log("BLOCK:",              block.number);
        console.log("CEREMONY_HELPER:",    address(ceremony));
        console.log("FHE_HELPER:",         address(fhe));
        console.log("PQ_HELPER:",          address(pq));
        console.log("ZK_HELPER:",          address(zk));
        console.log("===END===");
    }
}
