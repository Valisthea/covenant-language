# Covenant Language Support

**Covenant** is a privacy-first smart contract language that compiles to EVM bytecode. Its compile-time access control and privacy type system are real; the cryptographic primitives (FHE, ZK, post-quantum, cryptographic amnesia) are **testnet-only mocked stubs with no security yet**, see [STATUS](https://github.com/Valisthea/covenant-language/blob/main/STATUS.md). Write privacy-typed contracts without Solidity.

> **Continuously audited**, OMEGA V4 through V6 by Kairos Lab Security Research. Latest cycle (OMEGA V6, 5 July 2026): 6 Critical / 6 High / 5 Medium, all fixed the same day. Public report →

---

## Features

### Syntax Highlighting

Full TextMate grammar covering every Covenant construct:

- **Top-level constructs**: `module`, `record`, `token`, `ballot`, `counter`, `board`, `market`, `vault`, `registry`, `bridge`, `ceremony`
- **Privacy qualifiers**: `public`, `private`, `encrypted`, `sealed`, `hybrid`, `confidential`
- **Access control**: `only`, `when`, `given`
- **Types**: `amount`, `time`, `duration`, `hash`, `text`, `address`, `bool`, `ciphertext`, `map`, `shares`, `pq_key`, …
- **Control flow**: `match`, `let`, `return`, `if`, `else`, `for`, `each`, `try_action`, `catch`, `revert_with`
- **Annotations**: `@slot`, `@version`, `@selective_disclosure`, …
- **Comments**: `--` line comments
- **Operators**: full arithmetic, comparison, bitwise, assignment
- **Literals**: integers, hex (`0x…`), text strings, duration literals (`30 days`, `1 week`)

### Diagnostics

Real-time errors and warnings from the Covenant compiler, surfaced inline as you type. Covers:

- Parse errors
- Type mismatches
- Access control violations
- `@slot` annotation conflicts (E423)
- Unreachable code and dead assignments

### Hover Documentation

Hover over any identifier to see its type, visibility, and declaration site. Works for fields, actions, views, events, and imported symbols.

### Document Symbols

Full outline support, every `record`, `token`, `action`, `view`, `event`, and `field` appears in the VS Code Outline panel and breadcrumb.

### Language Configuration

- **Comment toggle** (`Ctrl+/` / `Cmd+/`), adds/removes `--` prefix
- **Auto-close**: `{`, `[`, `(`, `"` close automatically
- **Smart indentation**: increases inside `{` blocks, decreases on `}`
- **Code folding**: collapse any `{ }` block

---

## Requirements

Install the Covenant toolchain (compiler + language server). Today, build from source:

```sh
git clone https://github.com/Valisthea/covenant-language
cd covenant-language
cargo build --release --bin covenant-lsp   # then add target/release to PATH
```

> **Coming soon**, the hosted install channels below are not live yet:
> ```sh
> curl -sSL https://install.covenant-lang.org | sh   # coming soon
> cargo install covenant-cli                          # coming soon (crates.io)
> ```

The extension automatically discovers `covenant-lsp` from your `PATH`. No manual server configuration needed.

---

## Quick Start

1. Install the extension and the Covenant toolchain
2. Open or create a `.cov` file
3. Start writing, diagnostics, hover, and symbols activate immediately

```
-- hello.cov
record Hello {
    greeting: text

    action update(new_text: text) {
        greeting = new_text
    }

    view read returns text {
        greeting
    }
}
```

---

## Language Samples

### Token (ERC-20 equivalent)

```
token Coin {
    symbol:   "COIN"
    name:     "Covenant Coin"
    decimals: 18
    supply:   1_000_000 to deployer
}
```

### Access Control

```
record Vault {
    private balance: amount
    owner: address

    only owner
    action deposit(value: amount) {
        balance += value
        emit Deposited(value)
    }

    when balance > 0
    action withdraw(value: amount) {
        balance -= value
        emit Withdrawn(value)
    }

    view total returns amount { balance }

    event Deposited(value: amount)
    event Withdrawn(value: amount)
}
```

### Privacy + ZK

```
record PrivateVote {
    encrypted tally: map<address, choice>

    sealed
    action cast(encrypted vote: choice) {
        tally[caller] = vote
    }

    selective_disclosure
    view result returns choice {
        tally[caller]
    }
}
```

### Post-Quantum

```
record PqBoard {
    pq_signed
    action submit(data: bytes) {
        emit Submitted(data)
    }

    event Submitted(data: bytes)
}
```

---

## Extension Settings

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `covenant.lsp.path` | `string` | `"covenant-lsp"` | Path to the `covenant-lsp` binary. Override if not on PATH. |
| `covenant.lsp.enabled` | `boolean` | `true` | Enable or disable the language server entirely. |

---

## Supported File Types

| Extension | Language ID |
|-----------|-------------|
| `.cov` | `covenant` |

---

## Roadmap

- [ ] Go-to-definition
- [ ] Find all references
- [ ] Auto-completion (fields, actions, types)
- [ ] Semantic tokens
- [ ] Code actions (quick fixes for common errors)
- [ ] Formatter integration (`covenant fmt`)
- [ ] Debugger adapter

---

## Security & Audit

Covenant is continuously audited by **Kairos Lab Security Research** using the OMEGA offensive methodology. Covenant v0.6 was the first full audit (OMEGA V4, 5 phases):

| Severity | Found | Resolved |
|----------|-------|----------|
| Critical | 5 | ✓ 5 |
| High | 8 | ✓ 8 |
| Medium | 8 | ✓ 8 |
| Low | 6 | ✓ 6 |
| Info | 9 | ✓ 9 |

Key fixes: access control no-op (KSR-CVN-011), stale-memory precompile forgery (KSR-CVN-014), proxy initializer hijack (KSR-CVN-012), ceremony phase transition bypass (KSR-CVN-001).

**Latest cycle, OMEGA V6 (5 July 2026, compiler V0.9.3):** a fresh breadth-first sweep across the compiler, stdlib, and produced-contract surfaces found 6 Critical, 6 High, and 5 Medium severity defects, the largest single-cycle count since the V0.6 launch audit, including a `for each` loop that never actually iterated and an unbounded-recursion crash reachable from any `.cov` file this extension's language server opens. All 17 findings were fixed the same day, each with a new regression test. Full write-up: covenant-security-reviews/audits/2026-07-05-omega-v6-covenant-v0.9.2.

Full public report index: github.com/Valisthea/covenant-security-reviews

---

## Links

| | |
|---|---|
| Source | [github.com/Valisthea/covenant-language](https://github.com/Valisthea/covenant-language) |
| Documentation | [covenant-lang.org](https://covenant-lang.org) |
| Kairos Lab | [kairos-lab.org](https://kairos-lab.org) |
| Issues | [github.com/Valisthea/covenant-language/issues](https://github.com/Valisthea/covenant-language/issues) |
| Audit reports | github.com/Valisthea/covenant-security-reviews |

---

## License

Apache-2.0, see [LICENSE](LICENSE)
