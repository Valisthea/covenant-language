/// Return a short human-readable description for a known diagnostic or lint code.
/// Returns `None` for unknown codes.
pub fn lookup(code: &str) -> Option<&'static str> {
    Some(match code {
        // ── Lexer (E001–E010) ─────────────────────────────────────────────────
        "E001" => "Invalid character in source",
        "E002" => "Unterminated string literal",
        "E003" => "Invalid escape sequence in string",
        "E004" => "Unterminated block comment",
        "E005" => "Invalid numeric literal",
        "E006" => "Numeric literal out of range for amount",
        "E007" => "Use `--` or `(* *)` for comments, not `//` or `/* */`",
        "E008" => "Invalid identifier character",
        "E009" => "Reserved keyword used as identifier",
        "E010" => "Unexpected end of file in comment",

        // ── Parser (E100–E199) ────────────────────────────────────────────────
        "E100" => "Unexpected token",
        "E101" => "Expected construct keyword (record, token, vault, …)",
        "E102" => "Missing field name",
        "E103" => "Missing type annotation",
        "E104" => "Invalid action signature",
        "E105" => "Guard must appear before the action body",
        "E106" => "Duplicate field name in record/module",
        "E107" => "Missing closing brace",
        "E108" => "Invalid guard expression",
        "E109" => "Expected `returns T` for view",
        "E110" => "Unexpected visibility modifier — remove public/external/internal",
        "E111" => "Use `action` not `function`",
        "E112" => "Use `when cond` not `require(cond, msg)`",
        "E113" => "Bridge requires `anchored_on [...]` clause",
        "E114" => "Ceremony requires `guardians: N threshold: M`",

        // ── Resolver (E200–E299) ──────────────────────────────────────────────
        "E200" => "Undefined identifier",
        "E201" => "Duplicate definition",
        "E202" => "Circular reference",
        "E203" => "Field not found on type",
        "E204" => "Action not found",
        "E205" => "Guard references undefined identifier",
        "E206" => "Only one top-level construct per file",
        "E207" => "Unknown construct keyword",

        // ── Type checker (E300–E379) ──────────────────────────────────────────
        "E300" => "Type mismatch",
        "E301" => "Cannot use `amount` where `time` is expected (use a duration literal)",
        "E302" => "Cannot use `time` where `amount` is expected",
        "E303" => "Map key type not supported",
        "E304" => "Invalid argument count",
        "E305" => "Return type mismatch in view",
        "E306" => "Binary operator not defined for these types",
        "E307" => "Assignment to immutable field",
        "E308" => "Cannot index into non-map / non-array type",
        "E309" => "Encrypted type required here",

        // ── Privacy analysis (E380–E389) ──────────────────────────────────────
        "E380" => "Privacy domain violation: encrypted value leaked to plaintext context",
        "E381" => "reveal requires an encrypted field",
        "E382" => "pq_signed guard requires a pq_key argument",
        "E383" => "encrypted_when requires an encrypted bool condition",
        "E384" => "FHE branch has plaintext side effects",

        // ── IR / optimizer (E401–E423, W401–W410) ────────────────────────────
        "E401" => "Optimizer fixpoint not reached",
        "E403" => "IR invariant broken after optimization pass",
        "E404" => "Type inconsistency in IR",
        "E405" => "CSE type mismatch",
        "W401" => "Unused field",
        "W402" => "Unused local variable",
        "W403" => "Unreachable code",
        "W404" => "Event declared but never emitted",
        "W405" => "@precompute expression not evaluable at compile time — runs at runtime",

        // ── EVM backend (E501–E515, W501–W506) ───────────────────────────────
        "E501" => "EVM stack depth exceeded",
        "E507" => "Runtime bytecode exceeds 24 KiB limit",
        "E508" => "Deploy bytecode exceeds limit",
        "W504" => "Runtime bytecode is unusually large",

        // ── Linter detectors (C001, W003, I004, C100, …) ─────────────────────
        "C001" => "Reentrancy: state mutation after external call",
        "C002" => "Unchecked transfer return value",
        "W003" => "External call in a loop",
        "I004" => "Consider emitting an event on state change",
        "C100" => "Missing access guard on privileged action",
        "C101" => "Guard condition always true (tautology)",
        "W102" => "Overly broad `only owner` — consider a more specific principal",
        "W103" => "Redundant guard duplicates condition already checked",
        "I104" => "Consider using a typed error with revert_with instead of bare revert",
        "C300" => "Timestamp dependence: `now` used in a security-critical condition",
        "W301" => "Weak randomness source",
        "I302" => "Consider adding a duration check to time-sensitive actions",
        "C700" => "Ceremony: on_destroy block missing — secrets may not be destroyed",
        "C801" => "FHE branch has observable side effects (information leak)",
        "W1200" => "Gas: unbounded loop over storage array",
        "I1201" => "Gas: consider coalescing storage reads",

        _ => return None,
    })
}
