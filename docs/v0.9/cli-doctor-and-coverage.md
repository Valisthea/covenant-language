# `covenant doctor` + `covenant test --coverage` (V0.9 Sprint 41)

Sprint 41 of the V0.9 master plan focused on developer-experience CLI
ergonomics : a one-shot env-diagnostic command (`covenant doctor`) and a
first-cut coverage report (`covenant test --coverage`).

## `covenant doctor`

A single command that probes the local development environment and prints
a green ✓ / yellow ⚠ / red ✗ report. Inspired by `flutter doctor` and the
implicit env checks in Foundry.

### Output (human, default)

```
$ covenant doctor

  ✓ covenant                             0.9.0
  ✓ rustc                                rustc 1.95.0 (...)
  ✓ cargo                                cargo 1.95.0 (...)
  ✓ forge                                forge Version: 1.6.0-...
  ✓ cast                                 cast Version: 1.6.0-...
  ⚠ SEPOLIA_RPC_URL                      Sepolia RPC URL env var not set
  ⚠ ETHERSCAN_API_KEY                    Etherscan API key env var not set
  ✓ config/helper-addresses-v0.9.0.json  found at ...
  ✓ helpers/foundry.toml                 found at ...

Action items :
  1. SEPOLIA_RPC_URL, needed for Sepolia deploy / verify; export SEPOLIA_RPC_URL=...
  2. ETHERSCAN_API_KEY, needed for Sepolia deploy / verify; export ETHERSCAN_API_KEY=...
```

### Output (`--json`, for tooling)

```json
{
  "probes": [
    {"name": "covenant", "status": "ok", "detail": "0.9.0", "fix": null}...
  ]
}
```

### Probes shipped today

| Probe | What it checks | Failure mode |
|---|---|---|
| `covenant` | CLI version (always present) | n/a |
| `rustc` | `rustc --version`; warns if 1.5/1.6/1.7 | failed if absent |
| `cargo` | `cargo --version` | failed if absent |
| `forge` | `forge --version` (Foundry) | warning if absent |
| `cast` | `cast --version` (Foundry) | warning if absent |
| `SEPOLIA_RPC_URL` | env var set + non-empty | warning if absent |
| `ETHERSCAN_API_KEY` | env var set + non-empty | warning if absent |
| `config/helper-addresses-v0.9.0.json` | file exists in cwd | warning if absent |
| `helpers/foundry.toml` | covenant-helpers Foundry sub-project present | warning if absent |

### Exit code

`covenant doctor` always exits 0, it is **diagnostic**, not gating. A
future `--strict` flag could exit non-zero on any probe in `Failed` state
(useful for CI gates).

### Roadmap (V0.9.x)

  - `--strict` flag for CI integration
  - LSP binary presence check (`covenant-lsp` on PATH)
  - VS Code extension installation check
  - Disk-space estimation for build artifacts
  - Optional probe : RPC reachability test (one HTTP HEAD to the configured URL)

## `covenant test --coverage`

A first-cut, name-heuristic action coverage report for V0.9.0. Replaces
nothing (no prior coverage). Will be replaced by IR-instrumented per-test
execution tracking in V0.9.x.

### How it works

For every **non-test action** declared in the contract, check whether at
least one `test_*` action's name contains it (case-insensitive substring).
If so, the action is "covered" ; otherwise, it is "uncovered" and listed
explicitly.

### Example

```covenant
record Demo {
    n: amount = 0
    action set(value: amount) only caller { n = value }
    action reset() only caller { n = 0 }
    action increment() only caller { n += 1 }
    action test_set_works() when n == 0 {}
    action test_reset_brings_n_to_zero() when n == 0 {}
}
```

```
$ covenant test demo.cov --coverage
test test_set_works() ... ok
test test_reset_brings_n_to_zero() ... ok

test result: 2 passed; 0 failed

coverage: 2 / 3 actions covered (67%), name-heuristic
uncovered actions:
  - increment
```

`set` matches `test_set_works`. `reset` matches
`test_reset_brings_n_to_zero`. `increment` has no matching `test_*`, so
it is reported as uncovered.

### Limitations (V0.9.0)

  - **Pure heuristic.** `test_foo` "covers" `foo` even if its body never
    actually calls `foo`. The name match is a proxy for intent.
  - **View functions are not counted.** Test actions typically exercise
    views implicitly via the `when` guard pattern, so they don't need
    explicit coverage entries.
  - **Constructors are not counted.** Every test redeploys, so the ctor
    runs on every test by definition.
  - **No per-test breakdown.** The report rolls up across all tests.

### Roadmap (V0.9.x)

  - **IR instrumentation** : the IR layer already tracks action selectors
    per call; instrument the codegen to emit a side-channel log of
    "which action selector ran" per test, and the runner aggregates.
    This will replace the heuristic with ground-truth coverage.
  - **Per-test breakdown** : `--coverage --per-test` showing which test
    covered which action.
  - **HTML report** : `--coverage --html out/cov.html` like `cargo-tarpaulin`.
  - **CI gate** : `--coverage --min-pct 80` exits 1 if coverage drops
    below the threshold.
  - **Branch coverage** : after the action-level pass lands, extend to
    intra-action branch coverage (which arms of `when`/`if` ran).

## Why these two together (Sprint 41)

Sprint 40 hardened `covenant test` (per-test isolation) and `covenant fmt`
(`--check` semantic). Sprint 41 closes the dev-loop ergonomics gap : after
you fix a failing test, you want to know (a) "is my env still healthy ?"
(`doctor`) and (b) "did I leave any new actions untested ?" (`--coverage`).

Both are intentionally lightweight V0.9.0 implementations, a real CI gate
will land in V0.9.x once `--strict` and `--min-pct` ship.
