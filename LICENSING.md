# Licensing

**Covenant Language** ("Covenant" for short) is developed by [Kairos Lab](https://kairos-lab.org)
and is dual-licensed by component. This document is the authoritative statement of what is
licensed how.

## The split

| Component | Path | License |
|---|---|---|
| **Compiler & tooling** | all `crates/`, the CLI, LSP, linter, test runner, helper bridge (`helpers/src/`) | **Apache-2.0**, see [LICENSE](LICENSE) |
| **Language specifications** | the Styx privacy / PQ / ZK ERC drafts (in `docs/`) | **CC0-1.0** (public domain) |
| **Example contracts** | `examples/*.cov` | **CC0-1.0** (public domain), see [examples/LICENSE](examples/LICENSE) |

Why the split: the **compiler** is Apache-2.0 so it carries an explicit patent grant and a
trademark reservation while staying permissive enough for anyone to embed. The **specs and
examples** are CC0 so the language standard and its reference snippets can be copied, quoted,
re-implemented, and taught with zero friction, a standard nobody can freely reproduce is not a
standard.

## SPDX identifiers

- Compiler source (`.rs`): `SPDX-License-Identifier: Apache-2.0`
- Specifications and example contracts (`.cov`, spec docs): `SPDX-License-Identifier: CC0-1.0`

Per-file SPDX headers are being rolled out incrementally; where a file lacks a header, this
document and the component's directory govern.

## Third-party / vendored code

Vendored dependencies keep their own licenses and are **not** relicensed by this repository:

- `helpers/lib/forge-std/`: Foundry standard library, MIT / Apache-2.0 (upstream).

These paths are marked `linguist-vendored` in [.gitattributes](.gitattributes) so they do not
skew language statistics or the repository's detected license.

## Trademarks

"Covenant", "Covenant Language", "Covenant Lang", and the associated logos are trademarks of
Kairos Lab. **Apache-2.0 does not grant any right to use these marks** (Apache-2.0, Section 6).
You may state, factually, that your work uses or is compatible with Covenant. You may **not** use
the name or logo in a way that implies endorsement by, or official status from, Kairos Lab,
including naming a fork "Covenant" or presenting it as the official implementation.

## Future cryptography (V2.0)

The cryptographic primitives in this repository (FHE / post-quantum / ZK / VDF / Shamir) are
**mocked, testnet-only stubs with no security**: see [STATUS.md](STATUS.md). The real,
externally-audited cryptography runtime is a **separate, later release** and will ship from a
**separate repository under a separately-chosen license**. Nothing in this repository grants any
right to, or forecasts the license of, that future cryptographic implementation.

## Contributions

Unless you state otherwise, any contribution you intentionally submit for inclusion in the
Apache-2.0 components is provided under Apache-2.0, per Section 5 of that license. If a formal
Contributor License Agreement is introduced later, it will be documented in `CONTRIBUTING.md`
before any contribution is accepted under it.

---

*Questions about licensing: [admin@kairos-lab.org](mailto:admin@kairos-lab.org).*
