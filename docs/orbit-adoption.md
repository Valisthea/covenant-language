# Deploying Covenant to an Arbitrum Orbit chain

Written for teams running an Orbit chain who are evaluating whether Covenant is
usable on it. It states what works today, what does not, and what has never been
tested, so the evaluation can be done from facts rather than from a pitch.

Covenant is testnet only. Nothing here should be read as readiness to hold value.

---

## The short version

**The compiler has no notion of your chain.** Its target selector is a closed
list: a local mock chain, Sepolia, and one other testnet. Any other chain name is
a hard build failure, and there is no flag to supply custom helper addresses.
Saying "Covenant targets Orbit chains" would overstate it, and this document
exists partly to correct that.

**What works anyway is the part most people want.** Every construct that uses no
cryptography emits bytecode containing **no chain-specific address at all**. That
artifact is chain agnostic. You compile it, then deploy it with whatever tooling
you already use, and it runs on any EVM chain. That is how the first Covenant
contract beyond Ethereum reached an Orbit chain: a token exercising mint, burn
and an on-chain fee transfer, deployed and verifiable on a public testnet.

**The cryptographic constructs are a different story** and depend on four helper
contracts existing at fixed addresses on your chain. Details below.

---

## Support matrix

| Capability | On an Orbit chain | Basis |
|---|---|---|
| `token` (full ERC-20 surface synthesized) | **Works.** No chain-specific address emitted | Verified by reading the synthesizer and by an on-chain deployment |
| `nft` (ERC-721 surface synthesized) | **Works.** No chain-specific address emitted | Synthesizer emits only storage and event opcodes |
| `record`, `counter`, `board`, `market`, `module` | **Work**, but get no stdlib synthesis. You write the actions | These constructs pass through with no generated surface |
| `vault`, `ballot`, `bridge` | **Parse and compile**, but emit a "not implemented" warning and get **no synthesis** | The stdlib has no synthesizer for them yet |
| Native value transfer, `transfer <amt> to <addr>` | **Works.** Lowers to a plain `CALL` with an unscaled value word | No ETH assumption and no 18-decimal assumption exists in the lowering |
| `confidential token` (FHE) | **Requires helper contracts deployed on your chain** | Emits `PUSH20` immediates for the FHE helper |
| `ceremony` (cryptographic amnesia) | **Requires helper contracts deployed on your chain** | Emits `PUSH20` immediates for the ceremony helper |
| `registry` (post-quantum key registry) | **Does not compile at all**, on any target | Hard-blocked by diagnostic `E505` on a dynamic `bytes` path |
| `verified_by` and `pq_signed` guards | **Require helper contracts deployed on your chain** | Any construct carrying one pulls in a helper call |
| Any `encrypted` field, in any construct | **Requires helper contracts** | The dependency is per qualifier, not only per construct |

The last row matters more than it looks. A single `encrypted` field pulls an
otherwise plain construct into the helper-dependent path. Check per contract, not
per keyword. Every build artifact carries a `mockedCryptoPrimitives` field in its
metadata JSON listing exactly which mocked primitive families the emitted
bytecode calls. It is populated from the IR rather than from a build flag, so it
is ground truth about the bytecode. **If that field is empty, the contract
touches no mocked cryptography.** It is machine readable, so it is the right
thing to gate a pipeline on.

---

## Orbit-specific questions

### Custom gas token

No ETH assumption and no 18-decimal assumption exists at the bytecode level.
Native transfer lowers to a raw `CALL` carrying an unscaled value word, so it
follows whatever your chain's native asset is. The `decimals` you see in a
`token` declaration is that token's own ERC-20 value and is unrelated to the gas
asset.

**Untested empirically on a non-ETH gas chain.** There is no source-level
assumption to break, but nobody has run it. Treat it as unverified rather than
as safe.

### Gas schedules

**The compiler is not gas aware.** Every emitted call pushes a literal large gas
value. Under a custom L2 gas schedule this is untested and should be treated as
unknown.

### Helper contracts and CREATE2

The four helper addresses are `PUSH20` immediates baked into the emitted
bytecode. There is no runtime lookup, no constructor injection, and no on-chain
registry. Two consequences:

- Those addresses are deterministic only because they assume the Arachnid CREATE2
  factory at `0x4e59b44847b379578588920cA78FbF26c0B4956C` exists on the chain. On
  an Orbit chain without it, the prediction does not hold.
- There is **no runtime check that code exists at those addresses**. A call into
  an empty address fails at execution rather than at deployment, and worse, a
  `STATICCALL` into an empty address returns success with empty data, so a
  verification can read as passing when nothing ran.

Since v0.9.7 there is a compile-time check, but only where we know the answer.
`E533` refuses to build a contract that reaches a helper when the target's
helper contracts have not been confirmed deployed. Today that means:

| Target | Helpers | Build with a mocked primitive |
|---|---|---|
| `sepolia` | Deployed and verified, all four answer `eth_getCode` | Allowed |
| `mockchain` | Native precompiles, nothing to deploy | Allowed |
| `aster_testnet` | Never verified. The address manifest still records none | **Refused, E533** |
| Any other Orbit chain | No target exists | Not expressible |

That last row is the honest state. There is no generic EVM target, so there is
no way to build for an arbitrary Orbit chain and have the helper addresses be
right. A contract that touches no mocked primitive is unaffected: its bytecode
is byte-identical on every target, which is why the Robinhood Chain token
deployed without any of this mattering. If you operate an Orbit chain and want
the cryptographic constructs to work on it, the path is to deploy the four
helpers there and add a target with the verified addresses, not to guess.

### Custom precompiles

There is no supported way to register one. If your chain exposes precompiles at
addresses in the range this compiler uses for its local mock target, do not build
with the mock target and deploy the result.

### Permissioned deployment

The toolchain does not deploy anything. It writes artifacts to disk. So there is
no allowlist interaction on the compiler side. It does mean that on a permissioned
chain, the helper deployment step needs the same permission as any other
deployment, which only matters if you intend to use the cryptographic constructs.

---

## Recommended adoption path

Staged so that a governance or risk function can place it in policy.

**Stage 1, local.** Build and run against the local mock chain. Everything is in
process, nothing touches a network. Use this to decide whether the language fits
your problem at all.

**Stage 2, your testnet, plain constructs only.** Compile a contract that uses no
cryptography, confirm its build metadata reports no mocked primitives, and deploy
the artifact with your existing tooling. This is the path that is actually proven
on an Orbit chain today. Verify the deployed runtime bytecode against a local
recompile before trusting it.

**Stage 3, your testnet, cryptographic constructs.** Only after deploying the
helper contracts on your chain and confirming code exists at the addresses the
compiler emits. Expect to do that check yourself, because the compiler does not.
Treat everything in this stage as experimental: the primitives are deterministic
placeholders with no security property.

**Stage 4, production.** Not available. Covenant is testnet only, mainnet targets
are refused at compile time, and there has been no external audit. See the
security and audit roadmap for what would have to change.

---

## What we have not tested

Stated plainly so nobody infers coverage that does not exist.

- Any chain with a non-ETH native gas token.
- Any custom gas schedule.
- Any chain lacking the Arachnid CREATE2 factory.
- Any permissioned or semi-permissioned deployment flow.
- Any Orbit chain other than the one testnet deployment referenced above.

If you run Covenant on your Orbit chain and something in this document turns out
to be wrong, that is a finding we want. See `SECURITY.md`.
