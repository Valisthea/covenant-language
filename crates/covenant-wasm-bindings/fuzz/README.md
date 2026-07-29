# covenant-wasm-bindings: fuzz harness

Sprint 26 audit Phase 3 deliverable. Two targets:

| Target | Fuzzes | Suggested runtime |
|---|---|---|
| `compile_pipeline` | full lex → parse → resolve → typecheck → privacy → IR → opt → EVM codegen via `compile_to_evm` | **≥ 4 hours CPU** |
| `check_only` | frontend-only (no IR / no codegen), cheaper per iter, more coverage of lex/parse/resolve | ≥ 1 hour CPU |

## Setup (one-time)

```bash
# nightly toolchain is required by cargo-fuzz
rustup install nightly
rustup default nightly  # optional; you can also pass +nightly per-call
cargo install cargo-fuzz
```

## Run

```bash
cd crates/covenant-wasm-bindings

# 4-hour fuzz of the full pipeline
cargo +nightly fuzz run compile_pipeline -- -max_total_time=14400

# 1-hour fuzz of frontend-only check
cargo +nightly fuzz run check_only -- -max_total_time=3600
```

Each command prints periodic status. `cov: N` is branch coverage; `exec/s`
is throughput. Either should keep growing/holding steady, a sudden
collapse to 0 exec/s usually means a hang (which is itself a finding).

## Pass criteria

- **Zero panics** across the full run. Any panic = a finding.
- Coverage growth plateaus or trends up, a flat 0 means the corpus
  isn't reaching new branches; usually a sign the seed corpus is empty.

## What to do if a panic fires

1. Cargo-fuzz writes the crashing input to
   `fuzz/artifacts/<target>/crash-<sha>`.
2. Reproduce locally:
   ```bash
   cargo +nightly fuzz run compile_pipeline fuzz/artifacts/compile_pipeline/crash-<sha>
   ```
3. Open a finding `audits/<date>-omega-v4-covenant-v0.8/02-findings/KSR-CVN-NNN-<slug>.md`
   following the template.
4. Add the crashing input as a regression test in
   `crates/covenant-wasm-bindings/tests/smoke.rs` so the fix can be
   verified.

## Seed corpus (optional but speeds coverage growth)

```bash
mkdir -p fuzz/corpus/compile_pipeline
cp ../covenant-lexer/tests/fixtures/example_*.cov fuzz/corpus/compile_pipeline/
```

The 15 fixture contracts give the fuzzer a rich starting point of
syntactically valid Covenant source to mutate.

## Out-of-scope for fuzzing

- `chain_*` exports, these go through `wasm_bindgen` glue and require
  a JS host. The Sprint 26 PRELIM-006 fix added unit tests for the
  validators (which is most of the input-handling surface).
- WASM bundle size / determinism (I1), separate `sha256sum` check
  documented in `audits/.../phase2-report.md`.
