# covenant-testing

End-to-end test harness for the Covenant compiler. This crate is the
Phase 10 deliverable — it exercises the full pipeline
(source → lex → parse → resolve → typecheck → privacy → IR → stdlib → optimize
→ EVM bytecode) against a mock in-process EVM and deterministic precompiles.

## Why a custom EVM instead of `revm`?

Covenant currently targets Rust 1.75 (see `rust-toolchain.toml`), while the
modern `revm` / `alloy` releases require a newer MSRV. Rather than bump the
toolchain on a pre-V1 compiler, Phase 10 ships a *minimal* EVM interpreter
that covers exactly the opcode subset the Covenant backend emits and no more.
That keeps the harness self-contained, compile-fast, and auditable.

Scope:

* supported opcodes: `STOP`, `ADD`, `SUB`, `MUL`, `DIV`, `MOD`, `LT`, `GT`,
  `EQ`, `ISZERO`, `AND`, `OR`, `XOR`, `NOT`, `SHL`, `SHR`, `KECCAK256`,
  `ADDRESS`, `CALLER`, `CALLVALUE`, `CALLDATALOAD`, `CALLDATASIZE`,
  `CODECOPY`, `TIMESTAMP`, `NUMBER`, `CHAINID`, `POP`, `MLOAD`, `MSTORE`,
  `SLOAD`, `SSTORE`, `JUMP`, `JUMPI`, `JUMPDEST`, `PUSH0..PUSH32`,
  `DUP1..DUP16`, `SWAP1..SWAP16`, `LOG0..LOG4`, `CALL`, `STATICCALL`,
  `RETURN`, `REVERT`, `INVALID`;
* **no gas metering** (step limit only);
* `CALL` and `STATICCALL` only resolve against the Styx mock precompiles;
  plain-contract forwarding is not implemented.

## Mock precompiles

Deterministic stand-ins for the Styx suite (see `src/precompiles.rs`):

| Group    | Address range | Behavior                                                   |
|----------|---------------|------------------------------------------------------------|
| FHE      | `0x101..0x10F` | handle-based; ops execute on plaintext payloads             |
| Amnesia  | `0x120..0x123` | session counter + success-by-default                        |
| ZK       | `0x130..0x133` | success-by-default; `nullifier` returns `keccak(input)`     |
| PQ (8231)| `0x150..0x154` | `verify_*` succeeds unless `state.pq_force_fail` is set     |

> **Note** : early Covenant docs labeled the FHE / Amnesia / ZK address
> groups as "ERC-8227 / 8228 / 8229" — those numbers were aspirational,
> not officially registered. Since EIP-8228 was officially assigned to
> the Styx **Encrypted Token Standard** (a different specification, see
> [Valisthea/styx-erc-encrypted-token](https://github.com/Valisthea/styx-erc-encrypted-token)),
> Covenant docs use neutral group names instead. Only ERC-8231 (PQ key
> registry) is correctly registered.

## Test surface

Per-example scenarios live under `tests/`:

* `hello.rs` — minimal record deploys, dispatches, rejects unknown selectors
* `coin.rs` — ERC-20 conformance (14 scenarios)
* `open_ballot.rs` — ballot deploys, selector surface, warp-assisted
  `when` evaluation scaffolding
* `shielded_counter.rs` — bump path routes through the mocked FHE `add`
  precompile; handle table grows monotonically
* `quantum_board.rs` — post/verify selectors present; `pq_force_fail`
  propagates through the dispatch layer

The `smoke.rs` file also deploys every Basics example to guard against
codegen regressions.

## V0.1 caveats

A few compiler paths are still stubs. Tests document where this matters:

* calldata is not copied into function parameter slots yet, so
  `transfer(to, value)` reads `to = 0` and `value = 0` — round-trip state
  assertions (e.g. "approve 555 ; allowance == 555") are intentionally
  rewritten to validate dispatch/return behavior instead;
* the genesis mint for `supply: N to deployer` is deferred to V0.2, so
  `balanceOf(deployer) == 0` and first `transfer` reverts with
  `InsufficientBalance`;
* `text`/`bytes` params currently lower to a zero placeholder.

Each affected test carries a comment pinning the limitation it accepts.
