# `covenant test` + `covenant fmt` (V0.9 Sprint 40)

Sprint 40 of the V0.9 master plan focused on the test runner and the
formatter. Both commands existed in V0.8 ; Sprint 40 :

  - **`covenant test`** : added per-test isolation (each `test_*` action
    runs against a fresh `CovenantTestHarness`). Previously all tests
    in a file shared one deployed instance, and state set by one test
    leaked into the next.
  - **`covenant fmt`** : verified `--check` semantic (exit 1 on
    unformatted source) and documented the canonical-style intent.

## `covenant test`

### Discovery

Test actions are :
  - Any action whose name starts with `test_` (e.g. `test_initial_state`)
  - Any action carrying the `@test` annotation (V0.9.x)

Only **zero-argument** actions are picked up. Test actions with args
are silently ignored.

### Pass / fail semantics

A test **passes** when its action call returns successfully (no revert).
A test **fails** when the action :
  - reverts (any reason, guard fail, `revert_with`, panic)
  - aborts (compiler bug, EVM internal error)

### Assertion pattern (V0.9.0)

V0.9.0 doesn't yet ship inline `assert!`. Until V0.9.x adds the
stdlib testing API, the canonical pattern is :

```covenant
record MyContract {
    n: amount = 0

    -- Pass iff n == 0 at the start of the test.
    action test_initial_state() when n == 0 {}

    -- Mutate n. Test passes if no revert (e.g. arithmetic overflow,
    -- guard fail, etc.). With per-test isolation (Sprint 40), this
    -- mutation does NOT leak into the next test.
    action test_mutation_succeeds() {
        n = 100
    }
}
```

The `when` guard at the action signature acts as the assertion : if it
holds, the action runs (empty body, no side effects), returns Ok, and
the test passes. If it fails, the action reverts, and the test fails.

### Per-test isolation (Sprint 40)

Each `test_*` action runs against a **fresh** `CovenantTestHarness`.
The runner :

  1. Reads the source once.
  2. For each discovered test :
     - Creates a new `CovenantTestHarness::new()` (fresh MockChain).
     - Compiles + deploys the contract.
     - Calls the test action.
     - Records pass / fail.
     - Discards the harness.

Cost : one redeploy per test (~50ms for typical fixtures). Worth it
for the determinism guarantee, no test order dependency.

If you want to opt out (e.g. for performance benchmarks), V0.9.x will
add a `@shared_state` annotation. For V0.9.0, isolation is mandatory.

### Flags

| Flag | Behavior |
|---|---|
| `--filter <pattern>` | Run only tests whose name matches the substring (case-insensitive) |
| `--list` | List all discovered tests without running them |
| `--no-fail-fast` | Continue running after the first failure (default : stop) |
| `--gas-report` | Print per-test gas usage (V0.9.0 stub, gas not yet metered) |

### Example

```bash
$ cargo run --bin covenant -- test examples/test_isolation_demo.cov
test test_initial_n_is_zero() ... ok
test test_mutate_n_to_5() ... ok
test test_isolation_n_starts_zero_again() ... ok

test result: 3 passed; 0 failed
```

If isolation were broken, `test_isolation_n_starts_zero_again` would
fail (`n` would be 5 from the previous test). The fact that all three
pass demonstrates the isolation guarantee empirically.

## `covenant fmt`

### Style

Single canonical style (Rust `rustfmt` philosophy, no config). Driven
by `covenant_parser::printer`.

  - 4-space indent
  - 100-char soft line length
  - One blank line between top-level decls

### Limitations (V0.9.0)

  - **Comments are discarded.** The Covenant lexer does not preserve
    comments through the tokenize → parse → print round-trip. So
    `covenant fmt` on a commented file produces an uncommented file.
    Workaround : run fmt only on freshly-generated source, or wait for
    V0.9.x which adds comment preservation.

### Flags

| Flag | Behavior |
|---|---|
| (none) | Reformat in place |
| `--check` | Exit 1 if any file would be reformatted (CI gate) |
| `--diff` | Print unified diff to stdout instead of rewriting |
| `--stdin` | Read source from stdin, write formatted to stdout |

### CI gate

```bash
$ covenant fmt --check src/
would reformat: src/main.cov
error: source file(s) are not formatted, run `covenant fmt` to fix
```

Exit code 1 → CI job fails. Same pattern as `cargo fmt -- --check`.

## V0.9.x roadmap

  - **`covenant test --coverage`** : Sprint 41 will add per-action /
    per-view coverage reporting (uses the existing IR instrumentation).
  - **Stdlib testing API** : `assert!`, `assert_reverts!`, `assert_emits!`
    macros + chain helpers (`set_balance`, `advance_time`, `snapshot`,
    `revert_to`). Sprint 40.b, pending grammar work for the macro DSL.
  - **`covenant fmt` comment preservation** : requires lexer to retain
    trivia tokens. Mid-V0.9.x.
  - **`covenant test --watch`** : file-watcher mode, recompile + rerun
    on save. Sprint 41.
