# Covenant V0.9 Helper Contracts

Foundry sub-project that ships the four helper contracts the V0.9 compiler
emits CALL into for cryptographic primitives. See:

- `../docs/v0.9/precompile-bridge-architecture.md`: why these exist
- `../docs/v0.9/helper-interfaces.md`: interface specs
- `../docs/v0.9/address-resolution.md`: how the compiler routes to them
- `../config/helper-addresses-v0.9.0.json`: deployed address registry

## Contracts

| Contract | V0.9 status | V1.0 path |
|---|---|---|
| `CeremonyHelper` | **Real** state machine + destruction commitment | Wesolowski VDF added |
| `MockedFHEHelper` | **Mocked**, plaintext stored under handle | Real Zama TFHE |
| `MockedPQVerifier` | **Mocked**, parity check, not Dilithium | Solady PQ verifier |
| `MockedZKVerifier` | `verify` mocked, `nullifier` real | Real Halo2 verifier |

Mocked contracts revert on `block.chainid == 1` (Ethereum mainnet).
V0.9.0 ships testnet-only.

## Build + test

```sh
cd helpers
forge install foundry-rs/forge-std --no-commit  # if missing
forge build
forge test -vv
```

Expected: 30+ tests pass, 0 failures.

## Deploy to Sepolia

Prerequisites:

- `SEPOLIA_RPC_URL` env var (Alchemy / Infura / public)
- `DEPLOYER_PK` env var (private key with Sepolia ETH; never commit)
- `ETHERSCAN_API_KEY` env var (for `--verify`)

```sh
forge script script/Deploy.s.sol \
  --rpc-url $SEPOLIA_RPC_URL \
  --private-key $DEPLOYER_PK \
  --broadcast \
  --verify
```

Capture the printed addresses and feed them into
`../config/helper-addresses-v0.9.0.json` (replace the `TBD-after-deploy`
placeholders).

## Verify on Etherscan after deploy

If `--verify` flagged any contract as failed, re-run individually:

```sh
forge verify-contract <address> src/CeremonyHelper.sol:CeremonyHelper \
  --chain sepolia \
  --etherscan-api-key $ETHERSCAN_API_KEY
```

Repeat for `MockedFHEHelper`, `MockedPQVerifier`, `MockedZKVerifier`.

## CREATE2 salts (V0.9.0)

```
SALT_CEREMONY = keccak256("covenant-v0.9.0-ceremony")
SALT_FHE      = keccak256("covenant-v0.9.0-fhe")
SALT_PQ       = keccak256("covenant-v0.9.0-pq")
SALT_ZK       = keccak256("covenant-v0.9.0-zk")
```

V0.9.x bug-fix re-deploys with byte-identical init bytecode keep the same
addresses. If init bytecode changes, salt bumps to `covenant-v0.9.x-…` and
the registry version bumps too.

## Audit posture

- Slither + Mythril expected clean (no Critical/High)
- No `selfdestruct`, no `delegatecall`, minimal assembly (only `mstore`
  in `MockedPQVerifier.pqKeygenFromSeed`)
- All errors are typed custom errors; no plain string reverts
- All mutations emit events
- Reentrancy: not applicable, helpers make no external calls

## License

Apache-2.0. See `../LICENSE`.
