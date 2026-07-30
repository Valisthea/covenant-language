//! EVM backend diagnostic codes (E501-E520, W501-W510).
#![allow(dead_code)]

use covenant_diag::{DiagCode, Diagnostic, DiagnosticLevel, Span};

pub const E501_STACK_DEPTH: DiagCode = DiagCode(501);
pub const E502_UNKNOWN_OPCODE: DiagCode = DiagCode(502);
pub const E503_PRECOMPILE_UNSET: DiagCode = DiagCode(503);
pub const E504_STORAGE_OVERFLOW: DiagCode = DiagCode(504);
pub const E505_ABI_TYPE: DiagCode = DiagCode(505);
pub const E506_SELECTOR_COLLISION: DiagCode = DiagCode(506);
pub const E507_RUNTIME_TOO_LARGE: DiagCode = DiagCode(507);
pub const E508_DEPLOY_TOO_LARGE: DiagCode = DiagCode(508);
pub const E509_STRUCT_LAYOUT: DiagCode = DiagCode(509);
pub const E510_FHE_BRANCH_SIDE_EFFECTS: DiagCode = DiagCode(510);
pub const E511_ASSERT_ENCRYPTED: DiagCode = DiagCode(511);
pub const E512_LOG_TOO_MANY_TOPICS: DiagCode = DiagCode(512);
pub const E513_DYNAMIC_RETURN: DiagCode = DiagCode(513);
pub const E514_CIPHERTEXT_HANDLE: DiagCode = DiagCode(514);
pub const E515_UNRESOLVED_LABEL: DiagCode = DiagCode(515);
pub const E516_UNLOWERED_AMNESIA_OPCODE: DiagCode = DiagCode(516);
pub const E517_UNLOWERED_VDF_QUALIFIER: DiagCode = DiagCode(517);
pub const E518_UNLOWERED_BUILTIN_PREDICATE: DiagCode = DiagCode(518);
/// A division or remainder whose divisor is the literal `0`. The EVM's
/// `DIV`/`MOD` are total (`x / 0 == 0`), so this used to compile to bytecode
/// that silently produced 0. A statically-known zero divisor can never be
/// correct, so it is rejected at compile time rather than deferred to a
/// runtime revert.
pub const E519_DIV_BY_ZERO_LITERAL: DiagCode = DiagCode(519);
/// An opcode routed to a helper contract that has no method for it. The
/// selector table in `target.rs` covers 17 of the 31 opcodes reaching
/// `emit_precompile_call`; the rest used to fall back to the V0.8 namespaced
/// selector, which matches no function on the deployed helper. The helpers
/// have no fallback function, so the CALL always reverts: the contract
/// compiles clean, deploys clean, and bricks on first use.
pub const E520_HELPER_METHOD_MISSING: DiagCode = DiagCode(520);
/// A compile-time text constant longer than the 32 bytes the V0 return
/// encoder can emit. This used to be a bare `assert!`, so an ordinary
/// `name: "<33+ chars>"` aborted the whole compiler with an ICE instead of
/// pointing at the offending string. Found by the cargo-fuzz
/// `compile_pipeline` target.
pub const E521_TEXT_CONSTANT_TOO_LONG: DiagCode = DiagCode(521);
/// A nested map field (`map(K, map(...))`). The V0 map codegen lowers exactly
/// one `keccak(key ‖ slot)` level: the inner assignment `m[a][b] = v` was
/// never lowered at all (the write emitted ZERO SSTORE and the whole statement
/// was silently dropped), and the matching read hashed the key against the
/// outer entry's stored value: which is always 0 for a never-written nested
/// map: so `m[a][b]` returned 0. A well-typed nested-map program therefore
/// compiled to bytecode that silently discarded every write. Refuse to compile
/// rather than ship that. (OMEGA F09.)
pub const E522_NESTED_MAP_UNSUPPORTED: DiagCode = DiagCode(522);
/// `transfer <amount> from <src> to <dst>`, the three-operand form. The EVM has
/// no primitive that moves native value out of an account the executing contract
/// does not control: a `CALL` spends the *contract's own* balance. `emit_transfer`
/// destructured the operand list as `(operands[0], operands[2])`, so the `from`
/// operand was read by the parser, lowered by the IR builder, and then silently
/// discarded by codegen. The result compiled clean, emitted no diagnostic, and
/// paid `dst` out of the contract's balance while the source named in the source
/// text was ignored entirely. That is a silent miscompile on a value path, so the
/// form is refused until it has a faithful lowering.
pub const E523_TRANSFER_FROM_UNSUPPORTED: DiagCode = DiagCode(523);
/// A `hex` literal wider than 32 bytes. A single EVM PUSH carries at most 32
/// immediate bytes, and `push_n` computed the opcode as `0x60 + (len - 1)`
/// behind a `debug_assert!` the release binary compiles out. A 33-byte literal
/// therefore emitted `0x80` (DUP1) followed by the literal's own bytes as
/// executable instructions: a source-level constant became runtime code, and a
/// literal chosen as `CALLER PUSH4 0xfffffffe SSTORE` let any caller take over
/// the deployer-auth slot. A 256-byte literal truncated the length byte to 0,
/// emitting one PUSH0 while the size accounting still charged 257 bytes, which
/// desynchronised every label offset after it. Representing a wider constant
/// needs multi-word constant support (the same dynamic-`bytes` work tracked in
/// DEBT.md), so refuse rather than emit whatever `0x60 + (len - 1)` lands on.
pub const E530_HEX_CONSTANT_TOO_LONG: DiagCode = DiagCode(530);
/// A bare struct-typed field (`field cfg: Cfg`, as opposed to `[Cfg]`). The IR
/// builder has no lowering for `cfg.x = v`: the statement was dropped with no
/// instruction and no diagnostic, so every write to such a field vanished. The
/// read path is worse: `StructGet` treats its operand as a storage ADDRESS
/// (correct for a list element, where `ListGet` computes one), so `cfg.x` read
/// `SLOAD(SLOAD(slot) + 1)` and returned whatever the NEXT declared field
/// holds. A guard written as `when caller == cfg.who` therefore compared the
/// caller against an unrelated field that some other action can write. Faithful
/// support needs real multi-slot storage allocation for struct fields, so
/// refuse until that exists.
pub const E531_BARE_STRUCT_FIELD: DiagCode = DiagCode(531);
/// An `indexed` event parameter of a dynamic type (`text` / `bytes`). The ABI
/// spec says the topic is `keccak256(value)`, and the emitted ABI advertises
/// exactly that, but the constant path pushes a zero placeholder and there is
/// no dynamic-value hashing anywhere: every emit produced `topic1 = 0x00..00`,
/// so two logs carrying different tags were byte-identical in their topics and
/// a filter on `keccak256("alpha")` never matched. DEBT.md has claimed since
/// V0.1 that this is refused; it was not. Refusing it restores the documented
/// behaviour instead of shipping a topic that encodes nothing.
pub const E532_DYNAMIC_INDEXED_EVENT_PARAM: DiagCode = DiagCode(532);

/// A helper call emitted for a target whose helper contracts have never been
/// confirmed deployed.
///
/// `aster_testnet` reuses the CREATE2 addresses predicted for Sepolia, on the
/// reasoning that the Arachnid factory is deterministic across EVM chains.
/// That holds only where the factory is itself deployed, and nobody checked.
/// `config/helper-addresses-v0.9.0.json` still carries `"helpers": null` for
/// this target and a status reading "Verify in Sprint 42", which never
/// happened; there is no published Aster Chain testnet chain id and no public
/// EVM JSON-RPC endpoint to verify against.
///
/// So a contract using any mocked primitive built for this target carries
/// calls to four addresses that most likely hold no code. It deploys, and then
/// every guarded action reverts, or worse, a STATICCALL to an empty address
/// returns success with empty data and the primitive reads as a pass.
///
/// This is the same defect as the removed `evm` alias: a target that reads as
/// deployable while its helper addresses exist on no verified network.
/// Contracts that use no helper are unaffected and still build, since their
/// bytecode is identical on every target.
pub const E533_UNVERIFIED_HELPER_TARGET: DiagCode = DiagCode(533);

pub const W501_LARGE_MEMORY: DiagCode = DiagCode(501);
pub const W502_LARGE_STORAGE: DiagCode = DiagCode(502);
pub const W503_SELECTOR_NEAR_COLLISION: DiagCode = DiagCode(503);
pub const W504_LARGE_RUNTIME: DiagCode = DiagCode(504);
pub const W505_EVENT_NO_INDEX: DiagCode = DiagCode(505);
pub const W506_MANY_PARAMS: DiagCode = DiagCode(506);
pub const W507_DYNAMIC_RETURN_NOT_ENCODED: DiagCode = DiagCode(507);
/// `only caller`: a guard that lowers to `msg.sender == msg.sender`, i.e. a
/// tautology that imposes NO restriction. It is not a miscompile (the bytecode
/// faithfully implements "no restriction"), but it USED to be the one
/// degenerate principal that produced no diagnostic at all, while every other
/// unenforceable `only` principal already warns (W-class / KSR-CVN-011). This
/// closes that gap so an accidental no-op guard is no longer silent. (OMEGA F05.)
pub const W508_ONLY_CALLER_NOOP: DiagCode = DiagCode(508);
/// A non-indexed event parameter of a dynamic type (`text` / `bytes`). The
/// emitted ABI declares `string`/`bytes`, which a decoder reads as
/// offset + length + data, but the log data word is the same zero placeholder
/// the constant path pushes for any text. This is the non-indexed half of
/// E532 and it is only a warning for the same reason W507 is: emitting text in
/// an event is an ordinary, widely-used pattern, and hard-failing it would
/// make routine code uncompilable rather than fixing an edge case. Real
/// dynamic-`bytes` log encoding is tracked in DEBT.md.
pub const W530_DYNAMIC_EVENT_DATA_NOT_ENCODED: DiagCode = DiagCode(530);

fn warn(code: DiagCode, msg: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic {
        level: DiagnosticLevel::Warning,
        code,
        message: msg.into(),
        span,
        help: None,
    }
}

pub fn unknown_opcode(span: Span, name: &str) -> Diagnostic {
    Diagnostic::error(
        E502_UNKNOWN_OPCODE,
        format!("IR opcode `{name}` is not lowerable in the V0 EVM backend"),
        span,
    )
}

pub fn selector_collision(span: Span, a: &str, b: &str) -> Diagnostic {
    Diagnostic::error(
        E506_SELECTOR_COLLISION,
        format!("function selector collision between `{a}` and `{b}`"),
        span,
    )
}

pub fn runtime_too_large(span: Span, size: usize, max: usize) -> Diagnostic {
    Diagnostic::error(
        E507_RUNTIME_TOO_LARGE,
        format!("runtime bytecode size {size} exceeds limit {max}"),
        span,
    )
}

pub fn fhe_branch_side_effects(span: Span) -> Diagnostic {
    Diagnostic::error(
        E510_FHE_BRANCH_SIDE_EFFECTS,
        "FheBranch with observable side effects is not supported in V0 backend",
        span,
    )
}

pub fn assert_encrypted_not_lowerable(span: Span) -> Diagnostic {
    Diagnostic::error(
        E511_ASSERT_ENCRYPTED,
        "`AssertEncrypted` cannot be lowered in V0 (requires threshold-decrypt-then-revert)",
        span,
    )
}

pub fn dynamic_return_not_encoded(span: Span, ty_name: &str) -> Diagnostic {
    // OMEGA V6 (MED-003 fix): a `view`/`reveal` returning a genuinely
    // dynamic ABI type (`text`/`bytes`/a list) only has a real encoder for
    // a COMPILE-TIME-CONSTANT text literal (`emit_text_return`, used by
    // stdlib-synthesized `name()`/`symbol()`). Any other dynamic-typed
    // return value -- a runtime-read field, a computed value, `bytes`, or
    // a list -- falls through to the scalar path ("place value at memory
    // 0x00, return 32 bytes"), which returns just the raw handle/pointer
    // word instead of the offset+length+data a spec-compliant ABI decoder
    // (ethers.js, viem, a wallet) expects for that declared return type.
    //
    // This is deliberately a WARNING, not a hard error like the narrower
    // CRT-007/pq_key case: unlike that ERC-8231-specific construct, a plain
    // `view ... returns text { some_field }` is the single most common
    // pattern in the language (it's literally the "Hello World" example),
    // so hard-failing it would make routine, widely-relied-upon code
    // uncompilable rather than fixing an edge case. The audit's own
    // severity call was Medium specifically because of this, the fix for
    // "silently wrong with zero diagnostics" is "stop being silent," not
    // "block compilation" before dynamic-bytes/text/list ABI support
    // (tracked in DEBT.md) actually lands.
    warn(
        W507_DYNAMIC_RETURN_NOT_ENCODED,
        format!(
            "returning a non-constant `{ty_name}` value is ABI-encoded as a single raw word by \
             this release, not the offset+length+data a spec-compliant caller expects for a \
             dynamic type -- a caller decoding this per the published ABI will misread the \
             value. Real dynamic-`bytes`/`text`/list return encoding is tracked in DEBT.md."
        ),
        span,
    )
}

pub fn pq_key_abi_static_dynamic_mismatch(span: Span, fn_name: &str) -> Diagnostic {
    // OMEGA V6 (CRT-007 fix): `pq_key` (used by the ERC-8231 registry's
    // `register`/`key_of`) declares ABI type `bytes` (dynamic -- offset +
    // length + data) but `is_static_abi_ty` treats it as a plain 32-byte
    // word, so the parameter-read/return-encode paths do a raw
    // CALLDATALOAD/MSTORE of ONE word. Any spec-compliant caller
    // ABI-encoding a `bytes` argument places the dynamic-data OFFSET at
    // that word, not the key bytes -- so the contract silently registers
    // the constant 32 as the caller's "key", with no revert, no error.
    // Real dynamic-bytes ABI encoding is a larger, already-tracked item
    // (DEBT.md "dynamic bytes/string/T[] storage + ABI"); until that
    // lands, this raises E505 (which DEBT.md already names as the
    // intended interim signal: "raise E513_DYNAMIC_RETURN / E505_ABI_TYPE
    // ... so misuse fails loudly instead of mis-compiling") rather than
    // silently emitting bytecode that corrupts caller-supplied data.
    Diagnostic::error(
        E505_ABI_TYPE,
        format!(
            "`{fn_name}` uses `pq_key` (ABI type `bytes`, dynamic) in a position that this \
             release's codegen can only read/return as a single 32-byte word -- a \
             spec-compliant caller's ABI encoding would silently corrupt the value with no \
             error. Refusing to compile rather than shipping a compiler-caused key-corruption \
             bug. Real dynamic-`bytes` ABI support is tracked in DEBT.md."
        ),
        span,
    )
}

pub fn unlowered_builtin_predicate(span: Span, name: &str) -> Diagnostic {
    // OMEGA V6 (CRT-004 fix): `only <builtin_predicate>` guards for every
    // BuiltinPredicate variant beyond owner/admin/deployer/address used to
    // compile to an unconditional `push 1` ("Placeholder: return true.
    // Authorization semantics implemented later.") -- a complete,
    // undiagnosed authorization bypass for a first-class language guard
    // primitive. Refusing to compile (matching the existing
    // `unlowered_amnesia_opcode`/`unlowered_vdf_qualifier` pattern for
    // not-yet-implemented primitives) is strictly safer than shipping a
    // guard that silently passes for every caller.
    Diagnostic::error(
        E518_UNLOWERED_BUILTIN_PREDICATE,
        format!(
            "`only {name}` has no real EVM authorization check in this release -- the guard \
             would silently pass for every caller. Refusing to compile rather than shipping a \
             defeated access-control guard. Use `only owner`/`only admin`/`only deployer`/an \
             explicit `only <address>`, which ARE enforced, until a release implements this \
             predicate for real."
        ),
        span,
    )
}

pub fn unlowered_amnesia_opcode(span: Span, name: &str) -> Diagnostic {
    Diagnostic::error(
        E516_UNLOWERED_AMNESIA_OPCODE,
        format!(
            "IR opcode `{name}` has no EVM lowering in this release; emitting a REVERT stub. \
             Remove the primitive or wait for a release that lowers it (KSR-CVN-022)."
        ),
        span,
    )
}

pub fn unlowered_vdf_qualifier(span: Span) -> Diagnostic {
    Diagnostic::error(
        E517_UNLOWERED_VDF_QUALIFIER,
        "`@vdf_locked(delay)` has no EVM lowering in this release; \
         refusing to compile a time-locked action that would be instant at runtime (KSR-CVN-023)",
        span,
    )
}

pub fn unresolved_label(span: Span, name: &str) -> Diagnostic {
    Diagnostic::error(
        E515_UNRESOLVED_LABEL,
        format!("unresolved label `{name}` in final assembly"),
        span,
    )
}

/// A statically-known zero divisor. `DIV`/`MOD` on the EVM return 0 rather
/// than trapping, so this would otherwise compile to bytecode that silently
/// evaluates to 0: never what the author meant.
pub fn div_by_zero_literal(span: Span, op: &str) -> Diagnostic {
    Diagnostic::error(
        E519_DIV_BY_ZERO_LITERAL,
        format!(
            "`{op}` by a literal zero. The EVM returns 0 instead of trapping, so this \
             would silently evaluate to 0 rather than fail, rejected at compile time."
        ),
        span,
    )
}

/// An opcode with no method on the target's helper contract. Emitting the
/// V0.8 namespaced selector here produced a CALL that cannot dispatch, the
/// primitive is then neither real NOR mocked, and the action reverts on
/// chain. Refuse to compile: "deploy no code" beats "deploy broken code",
/// the same doctrine as E516/E517/E518.
pub fn helper_method_missing(span: Span, opcode: &str, target: &str) -> Diagnostic {
    Diagnostic::error(
        E520_HELPER_METHOD_MISSING,
        format!(
            "`{opcode}` has no method on the V0.9 helper contract for target `{target}`. \
             Emitting a call anyway would carry a selector that matches no function on \
             that contract, so the primitive would be neither real nor mocked and the \
             action would revert on first use. Build for `mockchain`, whose native \
             runtime implements this opcode, or wait for a helper release that adds the \
             method."
        ),
        span,
    )
}

/// A helper call for a target whose helpers were never verified deployed.
/// See [`E533_UNVERIFIED_HELPER_TARGET`].
pub fn unverified_helper_target(span: Span, opcode: &str, target: &str) -> Diagnostic {
    Diagnostic::error(
        E533_UNVERIFIED_HELPER_TARGET,
        format!(
            "`{opcode}` needs a helper contract, and the helper addresses for target \
             `{target}` have never been confirmed deployed. They are the addresses \
             predicted for Sepolia, reused on the assumption that the Arachnid CREATE2 \
             factory exists on that chain, which nobody verified: the address manifest \
             still records no helpers for this target. Emitting the call anyway would \
             produce a contract that deploys and then reverts on first use, or reads a \
             STATICCALL to an empty address as a passing verification. Build for \
             `sepolia`, where all four helpers are deployed and verified, or for \
             `mockchain`, whose runtime implements these opcodes natively."
        ),
        span,
    )
}

/// A text constant too long for the V0 single-word return encoder. Reported
/// instead of panicking: a 33-byte token name is ordinary user input, not a
/// compiler invariant violation.
pub fn text_constant_too_long(span: Span, len: usize) -> Diagnostic {
    Diagnostic::error(
        E521_TEXT_CONSTANT_TOO_LONG,
        format!(
            "text constant is {len} bytes; the V0 return encoder emits at most 32. \
             Shorten the string, or return it from a field the caller reads \
             separately. (Multi-word string returns need head/tail ABI encoding, \
             which V0 does not implement.)"
        ),
        span,
    )
}

/// A non-anonymous event with more than 3 `indexed` parameters. topic0 is
/// reserved for the event-signature hash, leaving room for at most 3 indexed
/// args. Beyond that, `emit` fell through to an unconditional `PUSH0 PUSH0
/// REVERT` (no `LOG5` opcode exists) AND the emitted ABI advertised >3 indexed
/// fields, which no spec-compliant decoder accepts. Reject at compile time so
/// the invalid event never reaches an artifact. (OMEGA F04.)
pub fn log_too_many_topics(span: Span, event: &str, indexed: usize) -> Diagnostic {
    Diagnostic::error(
        E512_LOG_TOO_MANY_TOPICS,
        format!(
            "event `{event}` declares {indexed} `indexed` parameters; a non-anonymous EVM \
             event allows at most 3 (topic0 is the event-signature hash, leaving 3 topics for \
             indexed args). This used to compile to an unconditional REVERT in `emit` and ship \
             an invalid ABI. Mark at most 3 parameters `indexed`."
        ),
        span,
    )
}

/// A nested map field. See [`E522_NESTED_MAP_UNSUPPORTED`].
pub fn nested_map_unsupported(span: Span, field: &str) -> Diagnostic {
    Diagnostic::error(
        E522_NESTED_MAP_UNSUPPORTED,
        format!(
            "field `{field}` is a nested map (`map(_, map(...))`), which this release's map \
             codegen cannot lower correctly: a nested write `{field}[a][b] = v` emitted no \
             SSTORE (the statement was silently dropped) and the matching read returned 0. \
             Refusing to compile rather than shipping a map that silently discards every \
             write. Flatten to a single map keyed by a composite/hashed key, or split into \
             separate maps."
        ),
        span,
    )
}

/// The three-operand `transfer ... from ... to ...`. See
/// [`E523_TRANSFER_FROM_UNSUPPORTED`].
pub fn transfer_from_unsupported(span: Span) -> Diagnostic {
    Diagnostic::error(
        E523_TRANSFER_FROM_UNSUPPORTED,
        "`transfer <amount> from <src> to <dst>` has no faithful lowering. A native-value \
         transfer compiles to a `CALL`, which spends the balance of the executing contract, \
         and the EVM offers no way to move value out of an account the contract does not \
         control. The `from` operand was previously parsed, lowered, and then dropped by \
         codegen, so this paid `<dst>` out of the contract's own balance while ignoring \
         `<src>`. Refusing to compile rather than shipping that. Use `transfer <amount> to \
         <dst>` to send the contract's own balance, or model the debit explicitly in \
         storage (for example a balances map) before transferring."
            .to_string(),
        span,
    )
}

/// A `hex` literal too wide for a single PUSH. See [`E530_HEX_CONSTANT_TOO_LONG`].
pub fn hex_constant_too_long(span: Span, len: usize) -> Diagnostic {
    Diagnostic::error(
        E530_HEX_CONSTANT_TOO_LONG,
        format!(
            "hex literal is {len} bytes; a single EVM PUSH carries at most 32. This used to \
             emit `0x60 + ({len} - 1)`, an unrelated opcode, followed by the literal's own \
             bytes as executable instructions -- a constant in the source became runtime \
             code. Split the value across several 32-byte constants (for example one field \
             per word), or hash it down to a 32-byte digest."
        ),
        span,
    )
}

/// A bare struct-typed field. See [`E531_BARE_STRUCT_FIELD`].
pub fn bare_struct_field(span: Span, field: &str, ty: &str) -> Diagnostic {
    Diagnostic::error(
        E531_BARE_STRUCT_FIELD,
        format!(
            "field `{field}` has struct type `{ty}` and is not held in a list, which this \
             release cannot lower: `{field}.<member> = v` emitted no instruction at all (the \
             write was silently dropped) and `{field}.<member>` dereferenced the field's \
             stored word as a storage address, returning the NEXT declared field's slot. \
             Refusing to compile rather than shipping a field that discards every write and \
             aliases its neighbour. Declare the members as separate top-level fields, or hold \
             the struct in a `[{ty}]` list, whose element access IS lowered."
        ),
        span,
    )
}

/// A dynamic `indexed` event parameter. See [`E532_DYNAMIC_INDEXED_EVENT_PARAM`].
pub fn dynamic_indexed_event_param(
    span: Span,
    event: &str,
    param: &str,
    ty_name: &str,
) -> Diagnostic {
    Diagnostic::error(
        E532_DYNAMIC_INDEXED_EVENT_PARAM,
        format!(
            "event `{event}` marks the dynamic parameter `{param}: {ty_name}` as `indexed`. \
             The ABI spec (and the ABI this compiler emits) says that topic carries \
             `keccak256({param})`, but nothing hashes it: every emit wrote `topic1 = \
             0x00..00`, so two logs with different values were indistinguishable and a topic \
             filter never matched. Drop `indexed` from `{param}` so the value at least \
             travels in the log data, or index a `hash` field you compute yourself."
        ),
        span,
    )
}

/// A dynamic non-indexed event parameter. See
/// [`W530_DYNAMIC_EVENT_DATA_NOT_ENCODED`].
pub fn warn_dynamic_event_data(span: Span, event: &str, param: &str, ty_name: &str) -> Diagnostic {
    warn(
        W530_DYNAMIC_EVENT_DATA_NOT_ENCODED,
        format!(
            "event `{event}` declares `{param}: {ty_name}`, published in the ABI as a dynamic \
             type (offset + length + data). This release writes a single placeholder word \
             there instead, so a caller decoding the log per the published ABI misreads it. \
             Real dynamic-`bytes`/`text` log encoding is tracked in DEBT.md."
        ),
        span,
    )
}

/// `only caller` is a tautological, no-op guard. See [`W508_ONLY_CALLER_NOOP`].
pub fn warn_only_caller_noop(span: Span) -> Diagnostic {
    warn(
        W508_ONLY_CALLER_NOOP,
        "`only caller` is a no-op guard: it lowers to `msg.sender == msg.sender`, always \
         true, so it imposes NO access-control restriction (0 CALLER checks emitted). If you \
         meant to restrict access use `only owner` / `only deployer` / `only <address>`; if \
         you intend no restriction, drop the guard to make that explicit.",
        span,
    )
}

pub fn warn_near_collision(span: Span, a: &str, b: &str) -> Diagnostic {
    warn(
        W503_SELECTOR_NEAR_COLLISION,
        format!("selectors of `{a}` and `{b}` differ by only one byte, near-collision"),
        span,
    )
}

pub fn warn_large_runtime(span: Span, size: usize) -> Diagnostic {
    warn(
        W504_LARGE_RUNTIME,
        format!("runtime bytecode size {size} exceeds 20 KB, approaching EIP-170 limit"),
        span,
    )
}
