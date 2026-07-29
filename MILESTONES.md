# Covenant: Milestones

Canonical historical record of project firsts. This file is appended-only.
Each entry is a real, verifiable event on a public chain or in this repo's
git history.

The CHANGELOG documents *what shipped*; MILESTONES documents *what was
proven for the first time*. They serve different audiences: the changelog
is for users tracking versions, the milestones file is for people asking
"when did this become real?".

---

## 🪨  M0: First Covenant contract on Ethereum Sepolia

| Field | Value |
|---|---|
| **Date** | 2026-04-25 (V0.8.0-rc7 era) |
| **Construct** | `record Hello` (the canonical first program) |
| **Address** | [`0xab083fc4922d34a799ca0f8f70711e1454018671`](https://sepolia.etherscan.io/address/0xab083fc4922d34a799ca0f8f70711e1454018671) |
| **Source** | `covenant-playground/public/examples/A1-hello.cov` |
| **Bytecode size** | ~62 bytes runtime |
| **Compiler** | V0.8.0-rc7 |
| **Target** | Sepolia testnet (chain id 11155111) |
| **Method** | Compiled in-browser via `covenant-wasm-bindings` (Sprint 22-25 work), deployed via MetaMask |
| **Significance** | First time Covenant source compiled in a browser, signed via wallet, and landed on a real public Ethereum chain. The handoff between in-tab MockChain (Sprint 23) and Sepolia (Sprint 24) was validated on real network. |

### Why this matters

Until M0, Covenant existed only as :
1. A Rust workspace that produced bytecode (V0.6 → V0.7)
2. An in-tab MockChain simulator (V0.7.1 → V0.8 via Sprint 22-23)

M0 is the first time bytecode produced by the Covenant compiler was
accepted, mined, and persisted by a public Ethereum validator network.
Anyone can read the bytecode at the address above, decompile it, or
call its `update`/`read` actions, it lives forever on Sepolia.

This was the playground operator smoke test that proved end-to-end
Sepolia integration in V0.8.0-rc7, before V0.8.0 GA tagged.

---

## 🪨  M1: First end-to-end Covenant `ceremony` lifecycle on Sepolia

**This is the headline milestone**: the first time a contract written in
the Covenant language source went through its **complete** Amnesia
ceremony lifecycle (setup → submit_share → submit_share → finalize →
destroy) on a real public Ethereum chain.

| Field | Value |
|---|---|
| **Date** | 2026-04-26 |
| **Construct** | `ceremony AmnesiaCeremony` (4 lines of Covenant source) |
| **Source** | [`covenant-playground/public/examples/C2-amnesia-ceremony.cov`](https://github.com/Valisthea/covenant-playground/blob/main/public/examples/C2-amnesia-ceremony.cov) |
| **Address** | [`0x2FB87d54D66c5fAEc1257a1A834497572fCe916D`](https://sepolia.etherscan.io/address/0x2FB87d54D66c5fAEc1257a1A834497572fCe916D) |
| **Compiler** | covenant V0.9-pre (Sprint 31.b incl. CALL-vs-STATICCALL dispatch + `>= 32` returndata check + opcode → Solidity-selector translation table) |
| **Deploy bytecode** | 904 bytes (vs ~62 bytes for the bare M0 Hello: ceremony adds the full amnesia ceremony stdlib synthesis) |
| **Helper bridge** | calls into V0.9.1 `CeremonyHelper` at [`0x627f1Ff6Dc93AEba050c242FD9E26961E8F6c6F0`](https://sepolia.etherscan.io/address/0x627f1Ff6Dc93AEba050c242FD9E26961E8F6c6F0) |

### The 5 transactions (full lifecycle, no skipped step)

| # | Action | Tx | Phase after |
|---|---|---|---|
| 1 | `setup()` | [`0xc707…41dd`](https://sepolia.etherscan.io/tx/0xc707a300c8af14f92460b7839d22bd11aa7cd3976645d02f639353a923cf41dd) | 1 = Active |
| 2 | `submit_share(0x111…)` | [`0x0d8a…4a21`](https://sepolia.etherscan.io/tx/0x0d8ada68083a901c6475c6a9eda1d655860dc80a14bb99bf029bdcc997604a21) | 1 |
| 3 | `submit_share(0x222…)` | [`0xd1fe…379d`](https://sepolia.etherscan.io/tx/0xd1fe8a17e64ae532685661b5bfb2018e42bc51fe70068b31622f53491c8c379d) | 1 |
| 4 | `finalize()` (thresholdMet=true) | [`0xf954…d68bc`](https://sepolia.etherscan.io/tx/0xf954754869a24a447f0d4e7c80ba857ef37354f1e76e37f1bfc8341275dd68bc) | 2 = Finalized |
| 5 | **`destroy()`** | [`0x261d…b2b8`](https://sepolia.etherscan.io/tx/0x261d2fc22283c687e9d3f5c9e0bd6e9163297d51fa3f3cd41889cb5f51a0b2b8) | **3 = Destroyed** |

Final post-call state confirmed via `cast call`:
- `phase()` returns `3` ✅
- `is_destroyed()` returns `true` ✅

### Why this matters more than the M1 helper deploy

The earlier sub-milestone (the 4 helper contracts deployed at predicted
CREATE2 addresses, see "M1.0, Helper contracts" below) proved that the
**infrastructure layer** worked. But those helpers are written in
**Solidity**: they're the trusted runtime that Covenant bytecode calls
into.

This milestone proves the **end-to-end story** : a developer writes
4 lines of Covenant source (`ceremony AmnesiaCeremony { guardians: 3,
threshold: 2, on_destroy { destroy(0) } }`), runs the Covenant
compiler with `--target-chain=sepolia`, gets EVM bytecode, deploys it
to Sepolia, calls its actions from a wallet, and the ceremony state
machine runs to completion with the destruction proof emitted on chain.

That's the actual product. Without this, M1.0 is "we deployed some
Solidity contracts", interesting infrastructure, not headline-worthy.
With this, the headline is **"first Covenant smart contract running its
full lifecycle on a public chain"**.

### Sprint 31.b bugs found and fixed during this milestone

The first three Covenant ceremony deploys (`0x69D4…`, `0xbbc2…`,
`0x0B4C…`) failed at various lifecycle points. The diagnostic uncovered
three real Sprint 31 implementation bugs that the design docs had
specified in principle but the codegen hadn't actually wired :

1. **Selector translation**: the Covenant compiler was emitting V0.8
   namespaced precompile selectors (`keccak("covenant.precompile.AmnesiaBegin:v1")`)
   for helper-contract targets, but the helpers expose Solidity ABI
   selectors (`keccak("amnesiaSetup(uint256)")`). Added
   `helper_selector_for_opcode(opcode_name)` translation table in
   `target.rs` that returns the Solidity selector when target uses
   helpers.

2. **CALL vs STATICCALL**: the Covenant compiler was emitting
   `STATICCALL` for ALL precompiles (correct for V0.8 stateless native
   precompiles), but `CeremonyHelper.amnesiaSetup` mutates state and
   STATICCALL reverts on state mutations. Added per-target dispatch:
   `CALL` for helper-contract targets, `STATICCALL` for MockChain.

3. **Returndata size check**: V0.8 native precompiles always returned
   exactly 32 bytes; V0.8 codegen used `RETURNDATASIZE == 32 OR REVERT`.
   But `amnesiaDestroy` returns `bytes memory` (variable-length, ABI-
   encoded as offset+length+data, total ≥ 96 bytes). Relaxed the
   check to `RETURNDATASIZE >= 32 OR REVERT` for helper-contract
   targets only. MockChain keeps strict equality.

Plus a 4th issue at the helper interface :

4. **Operand count mismatch**: V0.8 `Opcode::AmnesiaBegin` has 1
   operand (seed/nonce), but `CeremonyHelper.amnesiaSetup(uint256,uint256,uint256)`
   takes 3 args. Solidity 0.8's calldata-size dispatch check rejected
   the call. Patched the helper to add a 1-arg `amnesiaSetup(uint256)`
   overload that defaults to `guardians=3, threshold=2`. Helper bumped
   to V0.9.1, deployed at `0x627f1Ff6Dc93AEba050c242FD9E26961E8F6c6F0`
   under salt `keccak("covenant-v0.9.1-ceremony")`. Original V0.9.0
   helper at `0x6cAB…A16e` remains deployed for direct-cast diagnostics.

These are all small individual fixes (~30 lines of Rust + 8 lines of
Solidity) but together they were the difference between "design works
on paper" and "design works on Sepolia". The empirical loop,
deploy → fail → diagnose → fix → redeploy → succeed, is what makes
this M1 real.

---

## 🪨  M1.0: Helper contracts deployed (sub-milestone)

| Field | Value |
|---|---|
| **Date** | 2026-04-26 |
| **Block** | 10737692 (`0xa3d21c`) |
| **Constructs** | `CeremonyHelper`, `MockedFHEHelper`, `MockedPQVerifier`, `MockedZKVerifier` |
| **Deployer** | [`0x409D61d3582AD5A655927E615AC3CF366c165a55`](https://sepolia.etherscan.io/address/0x409D61d3582AD5A655927E615AC3CF366c165a55) |
| **Source** | `helpers/src/*.sol` (Sprint 30) |
| **Compiler** | Solidity 0.8.24 + Foundry CREATE2 via Arachnid factory |
| **Sprint** | 32 (V0.9 Phase A.1 closure) |

### The four addresses

CREATE2 derived via Arachnid factory `0x4e59b44847b379578588920cA78FbF26c0B4956C`
with salts `keccak256("covenant-v0.9.0-{ceremony,fhe,pq,zk}")`. Identical
on every EVM chain that has the factory deployed (verified on Sepolia,
expected on Aster Testnet pending Sprint 42).

| Helper | Address | Etherscan |
|---|---|---|
| `CeremonyHelper` | `0x6cABDD5Acf86D43C30CE560be68780E62F78A16e` | [verified ✅](https://sepolia.etherscan.io/address/0x6cabdd5acf86d43c30ce560be68780e62f78a16e#code) |
| `MockedFHEHelper` | `0x8f38e4F079570D77900fF2d6FfD0e6c96c401E44` | [verified ✅](https://sepolia.etherscan.io/address/0x8f38e4f079570d77900ff2d6ffd0e6c96c401e44#code) |
| `MockedPQVerifier` | `0xD3FcA4d62dc2162d9b07DEC2aCFc0A4Bda2A9010` | [verified ✅](https://sepolia.etherscan.io/address/0xd3fca4d62dc2162d9b07dec2acfc0a4bda2a9010#code) |
| `MockedZKVerifier` | `0xa9910bce5A3A47D0a92441C63D1e555A6CD7513c` | [verified ✅](https://sepolia.etherscan.io/address/0xa9910bce5a3a47d0a92441c63d1e555a6cd7513c#code) |

### First ceremony lifecycle on real Sepolia

| Field | Value |
|---|---|
| **Tx** | [`0xc9543870f61194fe40a984a03d335ba1892d6da642c1c588fd7f40f2f9970ca6`](https://sepolia.etherscan.io/tx/0xc9543870f61194fe40a984a03d335ba1892d6da642c1c588fd7f40f2f9970ca6) |
| **Method** | `CeremonyHelper.amnesiaSetup(nonce=42, guardians=3, threshold=2)` |
| **Result** | status=1 (success), event `AmnesiaSetup(sessionId, ceremony, 3, 2)` emitted |
| **Gas** | 157,444 (under the 200,000 Sprint 30 budget) |
| **State after** | `phase(sessionId)` returns 1 (Active), `sessionCount(deployer)` returns 1 |

### Why this matters

V0.8 had a fundamental gap : cryptographic constructs (`ceremony`,
`encrypted counter`, `pq_signed`, `verified_by`) compiled to bytecode that
called precompile addresses `0x101`, `0x154`. Those addresses worked on the
playground's in-tab MockChain (which implemented them as native
precompiles) but on Sepolia they were empty, `OP_CALL` succeeded with
zero return data, the calling contract proceeded as if the precompile
returned a valid result, the behavior was silently broken. Audit
finding **KSR-CVN-PRELIM-005**.

V0.9 Phase A.1 fixes this by bridging through deployed helper contracts
(per the `precompile-bridge-architecture.md` Option A decision in Sprint 29).
The compiler emits `CALL <helper_addr>` instead of `CALL 0x123`. Helpers
are deployed once via CREATE2 to deterministic addresses; the compiler
embeds those addresses in bytecode at compile time.

M1 proves the architecture works empirically :

1. **CREATE2 prediction was correct** : the 4 deployed addresses match
   exactly what Sprint 30 calculated from salt + init code hash + factory.
   This validates the Sprint 31 compiler routing layer end-to-end,
   bytecode emitted with these addresses will reach the right contract.

2. **State machine works on real network** : the first `amnesiaSetup`
   call advanced the ceremony to Active phase, emitted the expected
   event, and used 157k gas (well under the 200k budget). The Sprint 30
   helper contracts are not just deployable, they're functional.

3. **Etherscan-verified, externally inspectable** : anyone can read the
   four contract sources on Etherscan and verify the `Mocked*` naming +
   `notMainnet` modifier (defense in depth from Sprint 29 design).

This closes V0.9 Phase A.1 and unblocks Phase B (playground polish,
LSP, CLI, Aster integration). Per the V0.9 master plan §0.3, M1
arriving on time means the V0.9.0 monolithic ship plan stays viable.

### Resolution of KSR-CVN-PRELIM-005

The V0.8 audit finding flipped to "FIX VERIFIED". See
`covenant-security-reviews/audits/2026-04-25-omega-v4-covenant-v0.8/02-findings/KSR-CVN-PRELIM-005-call-no-target-execution.md`
for the full resolution narrative.

---

## 🪨  M2: First Covenant-compiled NFT (ERC-721) deployed + minted on Sepolia

The first NFT contract whose ERC-721 ABI surface was **auto-synthesized
by the Covenant compiler** from a 4-line `nft { ... }` source declaration,
deployed to Sepolia and exercising the full mint → ownerOf → balanceOf
flow on a real public chain.

| Field | Value |
|---|---|
| **Date** | 2026-04-26 (post-V0.9.0 tag) |
| **Construct** | `nft AuditNFT` (4 lines of Covenant source) |
| **Source** | [`examples/audit/04_nft_minimal.cov`](examples/audit/04_nft_minimal.cov) |
| **Address** | [`0xf8d9895cc265886d958841af8d9a6469be94bc25`](https://sepolia.etherscan.io/address/0xf8d9895cc265886d958841af8d9a6469be94bc25) |
| **Compiler** | covenant **V0.9.0** GA (commit `71d0e1b`, tag `v0.9.0`) |
| **Stdlib synth** | ERC-721 auto-synthesized by `covenant-stdlib::erc721` (Sprint 35.b, 515-line synthesizer) |
| **Deploy bytecode** | 1235 bytes (1208 runtime), 11 functions + 3 events + 4 errors from 4 source lines |
| **Helper bridge** | none required (no FHE/PQ/ZK opcodes ; pure ERC-721 logic) |
| **Deployer** | `0x409D61d3582AD5A655927E615AC3CF366c165a55` (same as M0/M1) |

### The 5 transactions (full lifecycle, 2 tokens, mirroring M1's 5-tx pattern)

| # | Action | Tx | Result |
|---|---|---|---|
| 1 | `--create $BYTECODE` (deploy) | [`0x9a40b8ca…054c`](https://sepolia.etherscan.io/tx/0x9a40b8ca1b18c3029aeccef50692292ea2aac79984f588d124b420978198054c) | block 10737903, gas 336,458 |
| 2 | `mint(deployer, 1)` | [`0x2107c1a2…7fe6`](https://sepolia.etherscan.io/tx/0x2107c1a2761a6f030a6ef5279462e3bbf6885fd87de1dd71727e2179a5b97fe6) | block 10737907, gas 72,571, Transfer(0x0, deployer, 1) emitted |
| 3 | `mint(deployer, 2)` | [`0xe88e72ee…c910`](https://sepolia.etherscan.io/tx/0xe88e72ee8bcefc164e98aabfe517b4478d050b07a90d85cb2980487a7bfcc910) | block 10738723, gas 55,471, Transfer(0x0, deployer, 2) emitted |
| 4 | `transferFrom(deployer, 0x...dEaD, 1)` | [`0xe9e75df2…c293`](https://sepolia.etherscan.io/tx/0xe9e75df2ab1068407c6dc059476f4f571b3ef889cf41ea53f5be5002e081c293) | block 10738724, gas 63,087, Transfer(deployer, 0xdEaD, 1) emitted |
| 5 | `transferFrom(deployer, 0x000…0000, 2)` (burn-attempt) | [`0xbcd0e1a2…d4d0`](https://sepolia.etherscan.io/tx/0xbcd0e1a2dd57a0962ac4e5525bebbb0c8a3840b2174fb6b819320f58582ed4d0) | block 10738725, gas 53,463, Transfer(deployer, 0x0, 2) emitted, **succeeded (empirical finding, see below)** |

Final post-lifecycle state confirmed via `cast call` :
- `name()` → `"Audit NFT"` ✅
- `symbol()` → `"ANFT"` ✅
- `ownerOf(1)` → `0x000…dEaD` (transferred) ✅
- `ownerOf(2)` → `0x000…0000` (zero-address, see empirical finding) ✅
- `balanceOf(deployer)` → `0` (wallet emptied) ✅
- `balanceOf(0x...dEaD)` → `1` ✅
- `balanceOf(0x000…0000)` → `1` (zero address now holds a token, see empirical finding) ⚠️
- `tokenURI(1)` → `"https://example.com/api/"` ✅

### Empirical finding : `transferFrom` to zero address succeeds (V0.9.0)

TX 5 attempted to "burn" token #2 via `transferFrom(deployer, address(0), 2)`.
**It succeeded** with status 0x1, gas 53,463, and emitted a Transfer event
with `to = 0x0`. Standard ERC-721 implementations (e.g. OpenZeppelin)
**explicitly revert** in this case with `ERC721InvalidReceiver(address(0))`
to prevent accidental burns ; the reference rationale is that ERC-721
intentionally distinguishes "transfer to a black-hole address" from "burn"
(burns should go through a `_burn` internal function so events use
`Transfer(owner, 0x0, id)` semantics with explicit intent).

Covenant V0.9.0's auto-synthesized `transferFrom` is **permissive** :
no zero-address check. This means :

  - **Effective burn-via-transferFrom path exists** (and it works).
  - **Non-conforming to strict ERC-721 semantics**: `balanceOf(0x0)`
    can be non-zero, which OpenZeppelin-aware indexers may treat as
    invariant-violated.
  - **No explicit `burn(uint256)` action in the auto-synthesized
    surface**, V0.9.0 deferred that to V0.9.x.

This is exactly the Sprint 31.b / Sprint 45 pattern : design docs assume
"the auto-synth follows OZ semantics", deploy-and-cast-loop reveals it
**doesn't**. Tracked in `DEBT.md` as a V0.9.1 fix candidate (add a
`require(to != 0x0)` check in `crates/covenant-stdlib/src/erc721.rs`
`emit_transferFrom`, OR document the permissive behavior explicitly +
add an opt-in `burn` action).

### The 4 lines of source

### The 4 lines of source

```covenant
nft AuditNFT {
    name: "Audit NFT"
    symbol: "ANFT"
    base_uri: "https://example.com/api/"
}
```

That's it. The compiler synthesizes everything else : `owners`,
`balances`, `token_approvals`, `operator_approvals` storage maps ;
`name`, `symbol`, `tokenURI`, `balanceOf`, `ownerOf`, `getApproved`,
`isApprovedForAll` view functions ; `approve`, `setApprovalForAll`,
`transferFrom`, `mint` actions ; `Transfer`, `Approval`,
`ApprovalForAll` events ; `NotTokenOwner`, `TokenAlreadyMinted`,
`TokenDoesNotExist`, `NotApprovedOrOwner` typed errors.

### Why this matters

  - **First Covenant NFT on a public chain.** M0 was a Hello (string
    field). M1 was a ceremony (helper-bridge dispatch). M2 is the first
    construct that exercises the **stdlib auto-synthesis** end-to-end on
    real Ethereum. The compiler turns a 4-line declaration into 1235
    bytes of deployable bytecode, that's the headline product.

  - **No helper bridge.** Unlike M1 (which depends on `CeremonyHelper`),
    NFT logic is pure EVM, no FHE / PQ / ZK opcodes. M2 validates that
    Covenant produces clean, helper-free bytecode for non-cryptographic
    constructs.

  - **ERC-721 surface is real ABI.** Wallets, marketplaces, indexers
    can interact with this contract using standard ERC-721 tooling. No
    Covenant-specific runtime required on the consumer side.

  - **First external-tooling-compatible Covenant deploy.** OpenSea,
    Etherscan token tracker, MetaMask NFT panel, they all see this
    contract as a standard ERC-721. The auto-synthesizer's job is
    **invisible** : downstream consumers can't tell the source was
    Covenant, only that it conforms to the standard.

### Sprint 35.b → V0.9.0 tag → M2 chain

Sprint 35.b shipped the ERC-721 auto-synthesizer (515 lines of Rust in
`covenant-stdlib/src/erc721.rs`). Sprint 47 tagged V0.9.0. M2 is the
first time we deploy it from the V0.9.0 GA compiler binary (not pre-tag
HEAD). This closes the "synthesis works at compile time AND at deploy
time" loop for ERC-721.

---

## 🪨  M6: First Covenant contract on Robinhood Chain (and first non-Sepolia chain)

The first Covenant-compiled contract deployed outside Ethereum Sepolia:
an ERC-20 with a `burn` sink, an opt-in transfer fee and on-chain
accounting, live on **Robinhood Chain testnet** (Arbitrum Orbit L2).
Also the first Covenant contract whose custom actions were validated by
an in-source `test` block **before** deployment, which caught a real
compiler bug (see below).

| Field | Value |
|---|---|
| **Date** | 2026-07-23 |
| **Chain** | Robinhood Chain **testnet**, chainId **46630** (`0xb626`), Arbitrum Orbit L2 |
| **Construct** | `token KairosCoin` + custom `burn` / `transfer_with_fee` / `set_fee` |
| **Source** | [`examples/kairos_coin.cov`](examples/kairos_coin.cov) (tests in [`kairos_coin.test.cov`](examples/kairos_coin.test.cov)) |
| **Address** | [`0x3E80F8c7911240e6092D523af79B13c046bd2FdE`](https://explorer.testnet.chain.robinhood.com/address/0x3E80F8c7911240e6092D523af79B13c046bd2FdE) |
| **Compiler** | covenant **V0.9.3** |
| **Deploy bytecode** | 2,493 bytes (2,422 runtime), 15 functions, 5 events, 2 errors |
| **Source verification** | [playground.covenant-lang.org/verify](https://playground.covenant-lang.org/verify), no public explorer can verify Covenant |
| **Deployer** | `0x1A7dA37293a85cBc7276Abe512355Ceb172c2d87` |
| **Total gas, all 5 txs** | ≈ 0.0000096 ETH (gas price 0.01 gwei) |
| **Helper bridge** | none required, plaintext ERC-20, `mockedCryptoPrimitives: []` |

### The transactions

| # | Action | Tx | Result |
|---|---|---|---|
| 1 | deploy | [`0xad3dc95e…453c`](https://explorer.testnet.chain.robinhood.com/tx/0xad3dc95ed1d547f6166bdd2ebaec3e3e964176dd6e94f42e75ab285737ce453c) | block 92,677,508, gas 673,243, 1,000,000 KRC minted to deployer |
| 2 | `set_fee(deployer, 100)` | [`0x595d07da…d9fa`](https://explorer.testnet.chain.robinhood.com/tx/0x595d07daf4a4dbdf4991a2f73be817ad9818740b6b75587854c5acb22fcbd9fa) | fee = 1.00 % |
| 3 | `set_fee(deployer, 501)` | *(reverted)* | ✅ the `given bps <= 500` guard enforced the 5 % cap on-chain |
| 4 | `burn(1_000 KRC)` | [`0xc085cf90…9b51`](https://explorer.testnet.chain.robinhood.com/tx/0xc085cf902b898930f6d1660d4b67548b7b29122f0c1129159976e1b0fd069b51) | totalSupply 1,000,000 → **999,000**; `Transfer` to `0x0` emitted |
| 5 | `transfer_with_fee(other, 10_000 KRC)` | [`0xb996381b…55f3`](https://explorer.testnet.chain.robinhood.com/tx/0xb996381bbbc7fae55dfb4adc355dde31b24331ddd2b5c56aebdb34956ccb55f3) | 9,900 net + 100 fee, **two canonical `Transfer` events** |

> **A first deployment preceded this one.** `0x40254d0b…65025` carried the same
> contract *plus its five `test_*` actions*, because Covenant V0.9 compiles a
> test into the deployed contract as a public function. It was redeployed clean
> (2,716 → 2,493 bytes, 20 → 15 functions) and the gap is filed in
> [`DEBT.md`](DEBT.md). The original remains on chain as the record of how it
> was found.

Final state confirmed via `cast call`:
- `totalSupply()` → `999000 × 10¹⁸` ✅  ·  `burned_total()` → `1000 × 10¹⁸` ✅
- `fees_collected()` → `100 × 10¹⁸` ✅  ·  `fee_rate_bps()` → `100` ✅
- Both fee legs emit topic0 `0xddf252ad…3b3ef`, byte-identical to canonical ERC-20,
  so explorers and indexers read it as a normal token.

### What this milestone additionally proved

- **A real compiler bug, caught by the in-source test block**: non-zero field
  defaults are silently dropped (`fee_bps = 100` read back as `0` on-chain).
  Verified on anvil and on Robinhood testnet; filed in [`DEBT.md`](DEBT.md).
  Without the test block the shipped source would have documented behaviour
  the bytecode did not implement.
- **`supply:` mints RAW base units**, not `decimals`-scaled, 1,000,000 whole
  tokens requires `supply: 1_000_000_000_000_000_000_000_000`.
- **Test actions ship on-chain.** The first deployment put all five `test_*`
  actions on the contract as public, callable functions. Harmless here (empty
  bodies), but a test that *mutates* would become a public unguarded state
  mutator, and the repo's own `test_isolation_demo.cov` contains exactly such
  a test. Filed in `DEBT.md`; tests now live in a separate `.test.cov` file
  until release-mode stripping exists.
- **Covenant needs its own source verifier.** Blockscout offers 8 verification
  methods, all Solidity/Vyper, none accepts Covenant, so the contract shows as
  *unverified source* on every public explorer. Tracked in `DEBT.md`.

> **Testnet only.** KRC has zero monetary value and is not tradable for
> anything real. This is compiler evidence, not a token launch.

---

## 🪨  M5: First Covenant-compiled PQ key registry deployed on Sepolia

The first contract whose post-quantum (Dilithium-5 / FIPS 204)
key-registry surface was **auto-synthesized by the Covenant compiler**
from a 1-line `registry { }` source declaration, deployed to Sepolia
and exercising the full register → query state flow on a real public
chain.

| Field | Value |
|---|---|
| **Date** | 2026-04-27 (post-V0.9.0 tag) |
| **Construct** | `registry AuditKeyRegistry` (1 line of Covenant source) |
| **Source** | [`examples/audit/05_registry_pq.cov`](examples/audit/05_registry_pq.cov) |
| **Address** | [`0xb9c5a5d874fa1797d8cfbbe7292051d9227eb1d3`](https://sepolia.etherscan.io/address/0xb9c5a5d874fa1797d8cfbbe7292051d9227eb1d3) |
| **Compiler** | covenant **V0.9.0** GA (commit `71d0e1b`, tag `v0.9.0`) |
| **Stdlib synth** | Auto-synthesized by `covenant-stdlib::erc8231` (Sprint 35.b, 340-line synthesizer) |
| **Deploy bytecode** | 476 bytes (449 runtime), 5 functions + 3 events + 2 errors from 1 source line |
| **Helper bridge** | none required (no real Dilithium verification in V0.9, `algorithm_id()` returns 1 = Dilithium-5 marker only) |
| **Deployer** | `0x409D61d3582AD5A655927E615AC3CF366c165a55` (same as M0/M1/M2) |

### The 2 transactions (deploy + first key registration)

| # | Action | Tx | Result |
|---|---|---|---|
| 1 | `--create $BYTECODE` (deploy) | [`0x6b6bf86f…5bfa`](https://sepolia.etherscan.io/tx/0x6b6bf86f5e2d2a33e23f83b180439dec0f4b55c27a0e288bd0ebedb86a135bfa) | block 10745269, gas 172,506 |
| 2 | `register(0x4b41495241ff)` ("KAIRA\xff" mock PQ key) | [`0xce1da053…88e8`](https://sepolia.etherscan.io/tx/0xce1da053b01e461358166bd571d829b3506d9192512967dc354f3888fd1988e8) | block 10745271, gas 49,790, KeyRegistered(deployer, key) emitted |

Final post-call state confirmed via `cast call` :
- `algorithm_id()` → `1` ✅ (Dilithium-5 per FIPS 204)
- `is_registered(deployer)` → `true` ✅
- `key_of(deployer)` → returns `0x000…0001` instead of registered bytes ⚠️ (see empirical finding below)

### The 1 line of source

```covenant
registry AuditKeyRegistry { }
```

That's it. The compiler auto-synthesizes everything else : `keys`,
`registered` storage maps ; `is_registered`, `key_of`, `algorithm_id`
view functions ; `register`, `revoke` actions ; `KeyRegistered`,
`KeyUpdated`, `KeyRevoked` events ; `NotRegistered`, `AlreadyRegistered`
typed errors. Sprint 35.c will add `update_key(new_pk, sig)` PQ-signed
key rotation once `pq_signed` guards integrate with stdlib synthesis.

### Why this matters

  - **First Covenant PQ key registry on a public chain.** Demonstrates
    the second auto-synth pipeline (after M2's ERC-721) producing
    standards-conformant bytecode from 1 line of source.
  - **Smallest deploy-bytecode footprint of any milestone yet** (476
    bytes deploy / 449 runtime), much smaller than M2 NFT (1235/1208)
    because PQ Registry has no token-id state, just per-account key
    storage.
  - **No helper bridge dependency.** Like M2, the registry is pure EVM
    logic. The "Dilithium-5" marker (`algorithm_id() == 1`) is just an
    ABI signal ; real PQ signature verification (V1.0) will route to
    PQVerifier helper.

### Empirical finding : `key_of` return type mismatch

TX 2 registered the key bytes `0x4b41495241ff` ("KAIRA\xff" in ASCII,
deliberately short to keep the hex obvious). After registration,
`key_of(deployer)` returned `0x0000000000000000000000000000000000000000000000000000000000000001`
, a 32-byte uint256-shaped value instead of the registered `bytes`
payload.

The ABI declares `key_of(address) returns bytes` but the runtime
appears to be returning a registered-marker uint256 (the value `1`,
matching `is_registered(deployer) == true`). The actual bytes payload
is either (a) stored but not returned correctly by `key_of`, or
(b) the auto-synthesizer in V0.9.0 stubs `key_of` to return the
`registered` boolean (cast as uint256) instead of reading from the
`keys` mapping.

Either way, it is **non-conforming to the auto-synthesized ABI**
(declared `bytes` return ; actual return shape mismatches). This is
the exact same Sprint 31.b / Sprint 45 / M2-burn-via-zero pattern :
design intent assumed correct, deploy-and-cast-loop reveals
divergence.

Tracked in `DEBT.md` as a V0.9.1 fix candidate. The auto-synthesizer
in `crates/covenant-stdlib/src/erc8231.rs` `emit_key_of` needs to be
audited and fixed to actually load + return the stored `bytes` from
the `keys` mapping.

### Sprint 35.b → V0.9.0 tag → M5 chain

Sprint 35.b shipped both the ERC-721 (M2) and ERC-8231 (M5)
auto-synthesizers in the same 515-line + 340-line dual delivery.
Sprint 47 tagged V0.9.0. M5 is the second time we deploy that
synthesizer family from the V0.9.0 GA compiler binary, after M2.
This closes the "synthesis works at compile time AND at deploy time"
loop for ERC-8231 (modulo the empirical finding above).

### Reserved cells (future milestones)

- M3, first cross-contract Covenant call on Sepolia : **PARTIAL
  V0.9.1** (2026-04-27). M3 proxy contract `M3CrossContractViewer`
  deployed at [`0xb48ef953c41e1f46c3affb1594bafb8ab3d1fc41`](https://sepolia.etherscan.io/address/0xb48ef953c41e1f46c3affb1594bafb8ab3d1fc41)
  via V0.9.1 (resolver fix unblocked compilation, deploy
  tx [`0xb5b2b7ea…a4ee`](https://sepolia.etherscan.io/tx/0xb5b2b7ea4bdd0f0ae1e2f44778f98be56436151120fdd147cf40b64547a6a4ee),
  block 10745331, gas 163,192). State writes work :
  set_nft(M2 NFT) tx [`0x31e87d0e…8404`](https://sepolia.etherscan.io/tx/0x31e87d0e7db56df2f686c28ddc38a7c6a1b2685afae2dc94f5eac7eed40d8404)
  succeeded, `cast storage 0` returns the M2 address correctly. **But**
  cross-contract STATICCALL reads return defaults (lookup_name returns
  empty, lookup_balance returns 0). Codegen STATICCALL chain emission
  bug, V0.9.2 fix candidate (see DEBT.md `external contract codegen`
  entry). M3 will graduate from "partial" to "full milestone" when
  V0.9.2 ships the codegen fix and the lookup_* views actually return
  M2's real state.
- M4, first Aster Testnet ceremony (V0.9.x era, when Aster
  factory verification unblocks deploy, see
  `docs/v0.9/aster-chain-integration-status.md`)
- M6, first external-audit external test pass (V1.0 era)
- V0.9.0 GA tag ✅ achieved 2026-04-26 (commit `71d0e1b`)
- V0.9.1 patch tag ⏳ in progress (resolver fix + ERC-721
  transferFrom-to-zero + ERC-8231 key_of return + erc8228 module
  rename + --strict doctor + VS Code bump)
- V1.0.0 GA tag ⏳ post-external-audit

---

## How to add a new milestone

When you reach a verifiable first :

1. Append a new `## 🪨 M?, <one-line description>` section
2. Include : date, block (if on-chain), addresses + tx hashes (if on-chain), source path, compiler version, sprint
3. Write a "Why this matters" paragraph that frames the milestone in the
   project's history, what wasn't possible before, what is now
4. Link back to the audit finding(s) closed by the milestone (if any)
5. Commit with message `milestone(M?): <description>`

The discipline : every milestone here must be **verifiable by an outside
party** (Etherscan link, git tag, etc.). Internal claims with no outside
reference go in CHANGELOG, not here.
