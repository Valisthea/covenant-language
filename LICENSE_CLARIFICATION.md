# License Clarification — FHE Technology & IP Position

**Last updated** : 2026-04-23
**Applies to** : Covenant compiler V0.7.0+
**Status** : Authoritative statement of Covenant's position on FHE technology and licensing

---

## TL;DR

Covenant is a language and a compiler. It does **not** implement FHE, and it does **not** depend on any FHE library (neither Zama's `tfhe-rs`, nor Microsoft SEAL, nor OpenFHE, nor any other). The compiler emits bytecode that calls FHE precompiles provided by the target chain.

When Covenant references "TFHE", we refer to the **academic TFHE scheme** (Chillotti, Gama, Georgieva, Izabachène — ASIACRYPT 2016 best-paper award, published in Journal of Cryptology 33(1)), which is a public-domain cryptographic scheme. We do **not** reference nor use Zama's commercial variants of TFHE (`tfhe-rs`, `concrete`, `fhEVM`), which are subject to Zama's patent licensing.

**Covenant does not require, and has never required, a Zama commercial patent license.**

---

## Section 1 — What Covenant Actually Is

Covenant is a declarative smart contract language with a Rust-based compiler. The compilation pipeline is:

```
Source (.cov) → Lexer → Parser → Resolver → Typechecker
             → Privacy Flow Analyzer → IR (SSA-form)
             → Optimizer → Backend Selection
             → { EVM bytecode | Aster Chain native | WASM (future) }
```

The FHE operations in Covenant source (`fhe_add`, `fhe_mul`, `encrypted<T>`, etc.) are compiled to **`STATICCALL` instructions** targeting chain-configured precompile addresses. Whether those precompiles exist and how they are implemented is **the responsibility of the target chain**, not of Covenant.

This is exactly analogous to Solidity's relationship with `keccak256`. Solidity allows you to write `keccak256(bytes)` as a first-class language construct, but Solidity does not implement keccak. The EVM provides keccak as a precompile or opcode. Solidity is a language; keccak is a primitive. Same with Covenant and FHE.

---

## Section 2 — What Covenant Does NOT Include

### 2.1 — No Zama code

Verification performed on 2026-04-23:

```bash
$ grep -r "tfhe\|zama" --include="Cargo.toml" covenant/
(no output — 0 matches)

$ grep -r "use tfhe\|use zama" --include="*.rs" covenant/
(no output — 0 matches)
```

No crate in the Covenant workspace depends on `tfhe`, `tfhe-rs`, `concrete`, `fhevm`, or any Zama library. No source file imports any Zama module. There is no Zama binary, source, or derivative work bundled with Covenant.

### 2.2 — No FHE implementation

Covenant does not contain any implementation of any FHE scheme. We do not have code that performs:
- TFHE bootstrapping
- FHE ciphertext addition / multiplication
- Key generation for FHE
- Threshold decryption
- Programmable bootstrapping

All of these are operations that Covenant's compiler **delegates to chain-side precompiles**. The compiler emits the instruction `STATICCALL <precompile_address>`, and whatever runs at that address is not part of Covenant.

### 2.3 — No binding to a specific FHE library

Covenant is **scheme-agnostic by design**. The whitepaper states this explicitly (doc1-whitepaper.md §4.7):

> "A conforming Covenant implementation MAY substitute schemes of equivalent or superior security and performance — but the developer-visible surface and the ABI contracts remain the same. Switching between TFHE and BGV-with-bootstrapping at the implementation level changes gas costs but does not break contract source."

This means a chain may implement Covenant's FHE precompiles using:
- The original TFHE reference implementation (Apache 2.0, `github.com/tfhe/tfhe`)
- Microsoft SEAL (MIT license, non-royalty-bearing patents)
- OpenFHE (Apache 2.0, `github.com/openfheorg/openfhe-development`)
- Lattigo (Apache 2.0)
- Any custom implementation

Covenant does not require any of these. It requires that the precompile produces the correct ABI-specified output for the given input. How it produces that output is an implementation detail of the chain.

---

## Section 3 — The TFHE Scheme vs. Zama's TFHE Variant

There is a common confusion between "TFHE" as a scheme and "TFHE" as marketed by Zama. Covenant only references the former.

### 3.1 — The academic TFHE scheme (public domain)

- **Authors** : Ilaria Chillotti, Nicolas Gama, Mariya Georgieva, Malika Izabachène
- **Initial publication** : ASIACRYPT 2016 — best paper award. Reference [11] in Covenant whitepaper.
- **Extended publication** : Journal of Cryptology 33(1), 2020.
- **Implementation** : `github.com/tfhe/tfhe` — Apache 2.0 license, maintained by the original authors and community.
- **IP status** : The mathematical scheme is academic publication (Asiacrypt 2016). It is in the public domain in the sense that anyone may implement it; no patent holder prevents implementation or commercial use. The original `github.com/tfhe/tfhe` is Apache 2.0, which explicitly grants commercial use rights.

Covenant's documentation cites this academic work. Covenant's compilation pipeline allows (and expects) chains to implement precompiles based on this scheme if they choose.

### 3.2 — Zama's TFHE variants (proprietary, patented)

- **Libraries** : `tfhe-rs`, `concrete`, `fhEVM`
- **License** : BSD-3-Clause-Clear (explicitly disclaiming any patent grants)
- **Patent posture** : "Zama's libraries are free to use under the BSD 3-Clause Clear license only for development, research, prototyping, and experimentation purposes. However, for any commercial use of Zama's open source code, companies must purchase Zama's commercial patent license."
- **What's patented** : Specific optimizations, implementations, and architectural choices Zama has made on top of the base TFHE scheme (e.g., specific programmable bootstrapping variants, their threshold decryption protocol, the fhEVM gateway architecture).

Covenant does not reference, depend on, or reproduce any part of these Zama variants.

### 3.3 — Summary

| Artifact | License | Covenant relationship |
|---|---|---|
| TFHE scheme (ASIACRYPT 2016 paper) | Public academic publication | Cited as reference [11]; informs IR design |
| `github.com/tfhe/tfhe` (original) | Apache 2.0 | Not used, but would be compatible |
| `github.com/zama-ai/tfhe-rs` | BSD-3-Clause-Clear (no patent grants) | Not used, not cited as dependency |
| `github.com/zama-ai/concrete` | BSD-3-Clause-Clear | Not used |
| `github.com/zama-ai/fhevm` | BSD-3-Clause-Clear | Not used |

---

## Section 4 — Current Position on Zama Patent License

**Covenant does not have, does not need, and does not seek a Zama commercial patent license.**

The reasons are direct:

1. **We do not use Zama's patented technology.** The patents cover specific optimizations to their variant of TFHE; we don't implement any of it.

2. **Our architecture doesn't require it.** Covenant is scheme-agnostic — a compiler of STATICCALL-emitting bytecode. No patented cryptographic operations happen inside Covenant.

3. **Users are not required to use Zama-licensed chains.** A user who deploys Covenant contracts on a chain using (for example) Microsoft SEAL or the original `tfhe` library has no Zama licensing obligation whatsoever.

4. **For users deploying on chains that use Zama technology** — that is the chain's licensing obligation, not Covenant's. Chains like Fhenix and Inco Network have their own licensing relationships with Zama; users benefit from those relationships when deploying on those chains.

This is not an adversarial posture toward Zama. We respect their work (it is cited in our whitepaper). We simply occupy a different layer of the stack.

---

## Section 5 — Commercial Deployment Guidance for Users

If you are deploying Covenant contracts commercially, here is how to think about FHE licensing:

### Step 1 — Which chain are you deploying to?

Your FHE licensing situation is determined by **the chain you deploy to**, not by Covenant.

### Step 2 — Check the chain's FHE implementation

Ask the chain:
- "What FHE implementation do your precompiles use?"
- "If it is Zama's variant (tfhe-rs, concrete, fhEVM), do you have a commercial Zama license that extends protection to your users?"
- "If it is a non-Zama implementation (OpenFHE, SEAL, Lattigo, original TFHE), can you document the license?"

### Step 3 — Act accordingly

| Situation | What you should do |
|---|---|
| Chain uses Zama tech, is Zama-licensed | Deploy freely; you are downstream-protected |
| Chain uses Zama tech, is NOT licensed | Do not deploy commercially until chain obtains license, or choose another chain |
| Chain uses non-Zama FHE (Apache 2.0, MIT, etc.) | Deploy freely; verify the chain's specific license terms match your use case |
| Chain publishes no information | Ask them; do not deploy without answer |

### Step 4 — For Aster Chain (Covenant's primary target)

Aster Chain is Covenant's primary target. As of V0.7 GA (April 2026), **Aster's FHE precompile implementation is pending**. When Aster publishes their precompile specification and licensing status, we will update this document.

### Step 5 — For Ethereum mainnet

There is no native FHE precompile in Ethereum mainnet. Covenant contracts using FHE primitives cannot be deployed to Ethereum mainnet directly. They can only be deployed on chains that have added FHE precompiles.

---

## Section 6 — Covenant's Policy Toward Zama

To be explicit and prevent any ambiguity:

**Covenant will not adopt, include, or depend on any Zama-licensed code.** This is a standing policy, effective at V0.7.0 GA and extending for the foreseeable future of the project.

This means:
- No `tfhe-rs` dependency in any Covenant crate
- No `concrete` dependency
- No `fhevm` dependency
- No Zama code forks
- No Zama-variant implementations within Covenant

If at some future point we want to add actual FHE computation inside the Covenant toolchain (e.g., for a WASM-compiled browser playground), we will use **Apache 2.0 or MIT-licensed** alternatives:
- Original TFHE (`github.com/tfhe/tfhe`, Apache 2.0)
- Microsoft SEAL (MIT)
- OpenFHE (Apache 2.0)
- Lattigo (Apache 2.0)

This policy preserves Covenant's scheme-agnostic architecture and keeps our users free from downstream licensing obligations originating from our choices.

---

## Section 7 — Frequently Asked Questions

**Q: If the academic TFHE scheme is public, why does Zama have patents?**

A: The mathematical scheme itself (as published in ASIACRYPT 2016) is public academic work. Zama's patents cover specific engineering innovations on top of that base: particular variants of programmable bootstrapping, their threshold decryption protocol, their gateway architecture, and other optimizations their research team has developed since 2019. Implementing the original ASIACRYPT 2016 TFHE does not infringe Zama's patents; implementing their specific optimizations does.

**Q: Why cite TFHE in your whitepaper if you don't implement it?**

A: For the same reason a language spec cites the EVM spec. We describe the cryptographic scheme our FHE primitives are designed to interface with. Citation is academic attribution; it does not create a dependency or a licensing obligation.

**Q: Would you accept a Zama license if they offered one for free?**

A: We have no need for one. Covenant does not use their code. A license grants rights we don't need. We would, however, welcome collaboration on technical specifications — e.g., ensuring Covenant's FHE precompile ABI is compatible with chains built on Zama's stack.

**Q: What if I want to build a Covenant chain using tfhe-rs?**

A: That's your decision and your licensing obligation. Your chain would need to obtain a Zama commercial license for the implementation choice. Covenant does not prevent this; Covenant does not endorse it either. The choice is yours as a chain builder.

**Q: Does this mean Covenant users should avoid Zama-based chains?**

A: No. Chains like Fhenix and Inco Network have their own Zama licensing relationships in place, and users deploying on those chains benefit from that coverage. The question is simply whether the chain has licensed what it uses.

**Q: What if Zama challenges this?**

A: We welcome clarification from Zama and have emailed them to confirm our understanding (see Section 9). If Zama identifies any specific issue, we will address it promptly. Our position is defensive and technical: we don't use their code, we don't implement their patented variants, and we don't misrepresent our relationship with their work.

---

## Section 8 — Verification Instructions

Anyone can verify the claims in this document:

### Verify no Zama dependency

```bash
git clone https://github.com/Valisthea/covenant-language
cd covenant

# Check Cargo.toml files
grep -r "tfhe\|zama" --include="Cargo.toml" .
# Expected output: (nothing)

# Check Rust source files
grep -r "use tfhe\|use zama" --include="*.rs" .
# Expected output: (nothing)

# Check Cargo.lock (if present)
grep -E "^name = \"(tfhe|zama)" Cargo.lock 2>/dev/null
# Expected output: (nothing)
```

### Verify scheme-agnostic architecture

Read `crates/covenant-evm-backend/src/codegen.rs` and note that every FHE operation is lowered to a `STATICCALL` to a precompile address. The implementation of the precompile is outside Covenant's scope.

### Verify FHE citation is academic

Read `doc1-whitepaper.md` reference [11]. It cites the ASIACRYPT 2016 paper (Chillotti et al.), not the Zama variant.

---

## Section 9 — Scheme-agnostic positioning

Covenant's position is that it requires no FHE-library license: it is a compiler that emits `STATICCALL` instructions to chain-configured precompiles and implements no cryptographic scheme itself (verifiable per Sections 7–8). This positioning is proactive and independent of any single FHE vendor — the choice of precompile implementation, and any licensing that choice entails, rests with the target chain, not with Covenant.

---

## Section 10 — Legal Notice

This document reflects Covenant's architectural posture and licensing analysis. It is provided for transparency and to help users make informed deployment decisions.

**This is not legal advice.** Users deploying commercially should consult qualified counsel for their specific circumstances.

**This document does not create any warranty.** The Covenant project disclaims any representation that deploying Covenant contracts on any particular chain is free of third-party IP obligations. Chain selection is the user's decision and responsibility.

**This document may be updated.** As Covenant evolves and as more chains adopt FHE precompiles, we will update this document to reflect the current state of the ecosystem.

---

## Changelog

- **2026-04-23** — Initial publication, concurrent with V0.7.0 GA launch.

---

## Contact

For questions about Covenant's IP position, architecture, or licensing:
- GitHub Issues: [github.com/Valisthea/covenant-language/issues](https://github.com/Valisthea/covenant-language/issues) (for public technical questions)
- Email: admin@kairos-lab.org (for sensitive legal matters, IP concerns)

For questions about specific chain implementations and their licensing:
- Contact the chain directly
- Zama for clarifications on their patents: hello@zama.ai

---

*Covenant is developed by Kairos Lab. Covenant compiler code is licensed under Apache-2.0. Covenant specifications are licensed under CC0-1.0. This document itself is CC0-1.0.*
