use regex::Regex;

/// Classify the best Covenant top-level construct for a Solidity source file.
pub fn classify_construct(source: &str) -> String {
    // Heuristics applied in priority order.
    if has(source, r"nonReentrant|ReentrancyGuard") && has(source, r"withdraw|ETH|msg\.value") {
        return "vault".into();
    }
    if has(
        source,
        r"transferEncrypted|balanceOfEncrypted|FHE|ciphertext",
    ) {
        return "confidential token".into();
    }
    if has(source, r"function transfer\b")
        && has(source, r"function approve\b")
        && has(source, r"function balanceOf\b")
    {
        return "token".into();
    }
    if has(source, r"vote|ballot|tally|poll") {
        return "ballot".into();
    }
    if has(source, r"\bcount\b|\bincrement\b|\+\+") && !has(source, r"mapping\(") {
        return "counter".into();
    }
    if has(source, r"emit.*Log|append|posts|messages|board") {
        return "board".into();
    }
    if has(source, r"ask|bid|order|match|orderbook|price") {
        return "market".into();
    }
    if has(source, r"register|registry|publicKey|address.*key|identity") {
        return "registry".into();
    }
    if has(source, r"bridge|lock|unlock|chain|cross.chain") {
        return "bridge".into();
    }
    if has(source, r"secret|share|guardian|threshold|ceremony|amnesia") {
        return "ceremony".into();
    }
    "module".into()
}

/// Apply all Solidity→Covenant transformations to `source`.
/// Returns (transformed_source, list_of_applied_transforms, list_of_todos).
pub fn transform(source: &str, construct: &str) -> (String, Vec<String>, Vec<String>) {
    let mut out = source.to_string();
    let mut applied: Vec<String> = Vec::new();
    let mut todos: Vec<String> = Vec::new();

    // ── T1: Remove import statements ─────────────────────────────────────────
    let before = out.clone();
    out = re(r#"import\s+"[^"]*";\n?"#, "", &out);
    out = re(r#"import\s+'[^']*';\n?"#, "", &out);
    if out != before {
        applied.push("remove import statements".into());
    }

    // ── T2: // comments → -- comments ────────────────────────────────────────
    // `(?m)` is load-bearing. Without it `$` anchors to the end of the whole
    // input rather than the end of each line, so the only comment that could
    // ever be rewritten was one on the final line, and every other comment
    // passed through as Solidity. The migration reported success while
    // handing back a file the compiler rejects on its first line.
    let before = out.clone();
    out = re(r"(?m)//(.*)$", "-- $1", &out);
    if out != before {
        applied.push("// → --".into());
    }

    // ── T3: /* */ block comments → (* *) ─────────────────────────────────────
    let before = out.clone();
    out = re(r"/\*", "(*", &out);
    out = re(r"\*/", "*)", &out);
    if out != before {
        applied.push("/* */ → (* *)".into());
    }

    // ── T4: contract Foo → <construct> Foo ───────────────────────────────────
    let before = out.clone();
    out = re(r"\bcontract\s+(\w+)", &format!("{} $1", construct), &out);
    if out != before {
        applied.push(format!("contract → {}", construct));
    }

    // ── T5: uint256 / uint → amount ──────────────────────────────────────────
    let before = out.clone();
    out = re(r"\buint256\b", "amount", &out);
    out = re(r"\buint\b", "amount", &out);
    if out != before {
        applied.push("uint256 → amount".into());
    }

    // ── T6: int256 / int → amount with TODO ──────────────────────────────────
    if has(&out, r"\bint256\b|\bint\b") {
        out = re(r"\bint256\b", "amount", &out);
        out = re(r"\bint\b", "amount", &out);
        applied.push("int256 → amount (with TODO)".into());
        todos.push("TODO(migrate): signed arithmetic → amount; signed types are V1.0".into());
    }

    // ── T7: string → text ────────────────────────────────────────────────────
    let before = out.clone();
    out = re(r"\bstring\b", "text", &out);
    if out != before {
        applied.push("string → text".into());
    }

    // ── T8: msg.sender → caller ──────────────────────────────────────────────
    let before = out.clone();
    out = out.replace("msg.sender", "caller");
    if out != before {
        applied.push("msg.sender → caller".into());
    }

    // ── T9: address(0) → zero_address ────────────────────────────────────────
    let before = out.clone();
    out = out.replace("address(0)", "zero_address");
    if out != before {
        applied.push("address(0) → zero_address".into());
    }

    // ── T10: block.timestamp → now ───────────────────────────────────────────
    let before = out.clone();
    out = out.replace("block.timestamp", "now");
    if out != before {
        applied.push("block.timestamp → now".into());
    }

    // ── T11: block.number → current_block ────────────────────────────────────
    let before = out.clone();
    out = out.replace("block.number", "current_block");
    if out != before {
        applied.push("block.number → current_block".into());
    }

    // ── T12: mapping(K => V) public xs → field xs: map<K, V> ────────────────
    let before = out.clone();
    out = re(
        r"mapping\((\w+)\s*=>\s*(\w+)\)\s*(?:public|private|internal|external)?\s+(\w+)\s*;",
        "field $3: map<$1, $2>",
        &out,
    );
    if out != before {
        applied.push("mapping → field xs: map<K, V>".into());
    }

    // ── T13: Remove visibility on remaining mappings ──────────────────────────
    let before = out.clone();
    out = re(r"mapping\((\w+)\s*=>\s*(\w+)\)", "map<$1, $2>", &out);
    if out != before {
        applied.push("mapping(...) → map<...>".into());
    }

    // ── T14: Remove modifier onlyOwner, nonReentrant (Solidity modifiers) ────
    let before = out.clone();
    out = re(
        r"modifier\s+onlyOwner\s*\(\s*\)\s*\{[^}]*_\s*;[^}]*\}",
        "",
        &out,
    );
    if out != before {
        applied.push("modifier onlyOwner → (removed; use `only owner` guard)".into());
    }

    let before = out.clone();
    out = re(r"\bnonReentrant\b", "", &out);
    if out != before {
        applied.push("nonReentrant → (removed; vault default)".into());
    }

    // ── T15: Remove payable ───────────────────────────────────────────────────
    let before = out.clone();
    out = re(r"\bpayable\b\s*", "", &out);
    if out != before {
        applied.push("payable → (removed; all actions are implicitly payable)".into());
    }

    // ── T16: Remove visibility modifiers on function/view declarations ────────
    let before = out.clone();
    out = re(
        r"\bfunction\s+(\w+)\s*\(([^)]*)\)\s*(?:public|external|internal|private)\s*(?:view|pure)?\s*",
        "action $1($2) ",
        &out,
    );
    out = re(
        r"\bfunction\s+(\w+)\s*\(([^)]*)\)\s*(?:view|pure)\s*(?:public|external|internal|private)?\s*",
        "view $1($2) ",
        &out,
    );
    out = re(
        r"\bfunction\s+(\w+)\s*\(([^)]*)\)\s*",
        "action $1($2) ",
        &out,
    );
    if out != before {
        applied.push("function → action / view".into());
    }

    // ── T17: constructor → action initialize ─────────────────────────────────
    let before = out.clone();
    out = re(r"\bconstructor\s*\(", "action initialize(", &out);
    if out != before {
        applied.push("constructor → action initialize".into());
    }

    // ── T18: require(cond, "msg") → when cond (guard hint) ───────────────────
    if has(&out, r"\brequire\s*\(") {
        out = re(
            r#"require\(\s*([^,)]+),\s*"[^"]*"\s*\)\s*;"#,
            "-- TODO(migrate): when $1",
            &out,
        );
        out = re(
            r"require\(\s*([^)]+)\)\s*;",
            "-- TODO(migrate): when $1",
            &out,
        );
        applied.push("require → when guard (manual placement required)".into());
        todos.push(
            "TODO(migrate): lift require() calls into action guard positions as `when` clauses"
                .into(),
        );
    }

    // ── T19: if/else in body → note the syntax difference ────────────────────
    // This used to tell the user that in-body if/else did not exist yet and
    // to restructure their code around `when` guards. It does exist, and has
    // since V0.9, so the advice sent people rewriting working control flow.
    if has(&out, r"\bif\s*\(") {
        todos.push(
            "TODO(migrate): in-body `if`/`else` is supported, but Covenant takes no \
             parentheses around the condition: write `if a == b { .. } else { .. }`. \
             For branching on encrypted values use `encrypted_when ... otherwise`."
                .into(),
        );
    }

    // ── T20: Remove remaining visibility modifiers left on state variables ────
    let before = out.clone();
    out = re(r"\b(public|external|internal|private)\s+(?=\w)", "", &out);
    if out != before {
        applied.push("visibility modifiers → (removed)".into());
    }

    (out, applied, todos)
}

fn has(source: &str, pattern: &str) -> bool {
    Regex::new(pattern)
        .map(|re| re.is_match(source))
        .unwrap_or(false)
}

fn re(pattern: &str, replacement: &str, source: &str) -> String {
    match Regex::new(pattern) {
        Ok(r) => r.replace_all(source, replacement).into_owned(),
        Err(_) => source.to_string(),
    }
}
