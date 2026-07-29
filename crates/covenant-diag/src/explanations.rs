//! Long-form prose explanations for diagnostic codes (V0.9 Sprint 38 Phase 38.1).
//!
//! Diagnostics emitted by the compiler carry a numeric `DiagCode` (e.g.
//! `E020`, `E421`, `W606`). The short message attached to a diagnostic is
//! intentionally tight: it has to fit inline next to the squiggle. The
//! long-form explanation lives here.
//!
//! Two consumers :
//!
//!   - **CLI** : `covenant explain E421` prints the long-form prose to
//!     stdout. Useful for quick lookup from the terminal.
//!
//!   - **LSP / IDE** : the LSP server attaches `codeDescription.href` to
//!     each diagnostic pointing at a future docs URL, AND exposes the
//!     same explanation text as a code action (`covenant.explain`) that
//!     IDEs render in the lightbulb menu.
//!
//! This file is **append-only**. Adding a new explanation is safe;
//! removing one risks breaking an outstanding LSP code action. If a code
//! becomes obsolete, mark its explanation as deprecated rather than
//! deleting it.
//!
//! Coverage today : the highest-frequency error codes from the playground
//! operator-test logs, plus a handful of warnings users hit during the
//! V0.9 Examples Gallery rewrite (Sprint 27 PRELIM-009 era).

use crate::DiagCode;

/// Long-form explanation for a single diagnostic code.
#[derive(Debug, Clone, Copy)]
pub struct Explanation {
    /// The numeric code, matching `DiagCode(N)`.
    pub code: u32,
    /// One-word category for grouping (`"lex"`, `"parse"`, `"resolve"`, ...).
    pub category: &'static str,
    /// Short, single-line summary. Same as the diagnostic.message text.
    pub summary: &'static str,
    /// Multi-line markdown prose with example + fix.
    pub body: &'static str,
}

impl Explanation {
    /// Pretty-print to stdout-friendly format.
    pub fn to_terminal_string(&self) -> String {
        format!(
            "E{:03} ({})\n\n{}\n\n{}",
            self.code, self.category, self.summary, self.body
        )
    }
}

/// Look up an explanation by `DiagCode`. Returns `None` if no prose is
/// registered for the code; the caller falls back to the inline message.
pub fn lookup(code: DiagCode) -> Option<&'static Explanation> {
    let n = code.0;
    EXPLANATIONS.iter().find(|e| e.code == n)
}

/// Look up by raw numeric code (for CLI `covenant explain 421`).
pub fn lookup_by_number(n: u32) -> Option<&'static Explanation> {
    EXPLANATIONS.iter().find(|e| e.code == n)
}

/// Iterate all registered explanations. Useful for `covenant explain --list`.
pub fn all() -> &'static [Explanation] {
    EXPLANATIONS
}

/// Curated registry of explanations for V0.9.0. Append-only.
const EXPLANATIONS: &[Explanation] = &[
    Explanation {
        code: 3,
        category: "lex",
        summary: "C-style line comment `//` is not valid Covenant syntax",
        body: r#"Covenant uses `--` for single-line comments (Haskell / Ada
style), not `//`. This is intentional: Covenant deliberately diverges
from C-family syntax to signal a different mental model.

Wrong:
    // counter for active sessions
    field counter: amount

Right:
    -- counter for active sessions
    counter: amount

For multi-line comments, use `(* ... *)` (Pascal / OCaml style; nesting is
allowed):
    (* outer (* nested *) outer *)

If you're migrating from Solidity, see the cheat sheet at
docs.covenant-lang.org/migration/from-solidity for the full mapping."#,
    },
    Explanation {
        code: 20,
        category: "parse",
        summary: "expected `}`, got end of file",
        body: r#"The parser reached the end of the file while still inside
an open block. This usually means a `{` is missing its closing `}`, or a
nested block (action body, if-block, etc.) didn't get terminated.

Quick check: every `{` should have a matching `}` at the same indentation
level. If you intentionally split a record across multiple lines, make
sure the final `}` is at the start of its own line.

Editor tip: most editors highlight matching braces, place your cursor
on the `{` that opens the block and look for the missing `}` below."#,
    },
    Explanation {
        code: 421,
        category: "lint",
        summary: "expected `--` (or `(* *)`), got `//`",
        body: r#"Same root cause as E003: Covenant uses `--` for line
comments, not `//`. The lint detector flags this with a quick fix that
rewrites `//` → `--` automatically.

In your editor (with the Covenant LSP installed): position the cursor on
the `//` and press the lightbulb menu → "Replace `//` with `--`"."#,
    },
    Explanation {
        code: 503,
        category: "evm-codegen",
        summary: "precompile address unset for this opcode",
        body: r#"The EVM backend tried to emit a `CALL` to a precompile
address that was not configured in the active `PrecompileMap`. This is a
compiler bug: every cryptographic opcode (FHE / PQ / ZK / Amnesia) must
have an address in `PrecompileAddresses` before codegen runs.

If you hit this in an end-user contract, please file a bug:
github.com/Valisthea/covenant-language/issues, include the opcode name and the
target chain you're compiling for."#,
    },
    Explanation {
        code: 510,
        category: "evm-codegen",
        summary: "FHE branch has side effects",
        body: r#"A `cmux`-style encrypted conditional (`encrypted_when ...
otherwise ...`) cannot have side effects in either branch. Both branches
are evaluated unconditionally on encrypted operands; a side-effecting
branch would execute even when its condition is false.

Wrong:
    encrypted_when caller_has_role { count += 1 } otherwise { 0 }

Right (move the side effect outside, store the encrypted result):
    let inc = encrypted_when caller_has_role { 1 } otherwise { 0 }
    count += inc

If the branch needs to perform a state-changing action, restructure so
the FHE selection produces a value, then act on it after the encrypted
selection completes."#,
    },
    Explanation {
        code: 601,
        category: "stdlib-synthesis",
        summary: "user-declared function conflicts with auto-synthesized one",
        body: r#"You declared an action or view whose name matches one
the stdlib synthesizer was about to inject (e.g. `transfer` inside a
`token` construct). In `strict_conflict_detection` mode (the default),
the synthesizer aborts; in permissive mode it would skip its own version
and use yours, but the strict mode catches the case where you accidentally
shadow the standard.

Two common fixes :
  1. If you meant to override : pass `strict_conflict_detection: false`
     to `StdlibConfig` (only available via `covenant build` API; not
     a CLI flag yet: V0.9.x).
  2. If you didn't mean to override : rename your function. Common
     mistake : a custom `transfer` that doesn't follow ERC-20 semantics."#,
    },
    Explanation {
        code: 606,
        category: "stdlib-synthesis",
        summary: "standard-interface synthesizer not yet implemented for this construct",
        body: r#"The construct (e.g. `nft`, `registry`, `vault`, `bridge`)
is recognized by the parser and produces a deployable contract, but the
auto-synthesis of its standard ABI surface (ERC-721, ERC-8231, etc.) is
not yet implemented in the V0.9 release.

For V0.9.0 :
  - `nft`      → ERC-721 surface auto-synthesized (Sprint 35.b)
  - `registry` → ERC-8231 surface auto-synthesized (Sprint 35.b)
  - `vault`    → not yet (V0.9.x backlog)
  - `bridge`   → not yet (V0.9.x backlog)

If your construct's surface IS synthesized but you still see this warning,
make sure the corresponding `StdlibConfig::synthesize_X` flag is `true`
(default). The bare construct still compiles to a deployable no-op
contract regardless."#,
    },
    Explanation {
        code: 7000, // arbitrary high range for V0.9 Sprint 31 introductions
        category: "target",
        summary: "invalid --target-chain value",
        body: r#"The `--target-chain` flag accepts:

  Backend dispatch :
    aster                          → Aster native backend

  EVM bytecode dispatch (V0.9 Sprint 31) :
    mockchain | mock | evm         → in-tab MockChain (V0.8 default)
    sepolia                        → V0.9 helpers on Ethereum Sepolia
    aster_testnet                  → V0.9 helpers on Aster Testnet

  Rejected :
    mainnet | ethereum             → V0.9 ships testnet-only; mainnet
                                     helpers land in V1.0 post external
                                     audit. Compiler refuses this value
                                     at parse time.

If your target name has a typo, this error catches it. If you want a
chain that's not listed (e.g. Polygon, Optimism), open an issue, 
V0.9.x can add new EVM-compatible testnets cheaply once the helper
contracts deploy there."#,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_known_codes_succeeds() {
        for e in EXPLANATIONS {
            assert_eq!(lookup(DiagCode(e.code)).unwrap().code, e.code);
        }
    }

    #[test]
    fn lookup_unknown_code_returns_none() {
        assert!(lookup(DiagCode(999_999)).is_none());
    }

    #[test]
    fn terminal_string_includes_code_and_summary() {
        let e = lookup(DiagCode(421)).unwrap();
        let s = e.to_terminal_string();
        assert!(s.contains("E421"));
        assert!(s.contains("expected `--`"));
        assert!(s.contains("Replace") || s.contains("rewrites")); // body content
    }

    #[test]
    fn all_explanations_have_nonempty_body() {
        for e in EXPLANATIONS {
            assert!(!e.body.trim().is_empty(), "E{} has empty body", e.code);
            assert!(
                !e.summary.trim().is_empty(),
                "E{} has empty summary",
                e.code
            );
        }
    }
}
