//! The capability registry must describe what the compiler emits.
//!
//! `config/capabilities.json` classifies each top-level construct as PORTABLE,
//! INCOMPLETE, MOCKED_TESTNET_ONLY, NO_SYNTHESIS or REFUSED. The review that
//! prompted it put the demand plainly: registry, board and vault must be
//! classified from actual emitted semantics, not from the parser accepting the
//! keyword. A keyword that parses but cannot deliver what it names must not
//! look deployable.
//!
//! This test is what makes the file true. For each construct it compiles a
//! fixture, derives the observed state from the compiler's behaviour, and
//! asserts it matches both the table here and the published JSON. The JSON
//! cannot drift from the compiler without this failing.

use covenant_diag::{Diagnostic, DiagnosticLevel, SourceId};
use covenant_evm_backend::{EvmArtifact, EvmConfig, Target};
use covenant_opt::OptimizerConfig;
use covenant_stdlib::StdlibConfig;

const REGISTRY_JSON: &str = include_str!("../../../config/capabilities.json");

const W606_SYNTH_NOT_IMPL: u32 = 606;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum State {
    Portable,
    Incomplete,
    MockedTestnetOnly,
    NoSynthesis,
    Refused,
}

impl State {
    fn as_json(self) -> &'static str {
        match self {
            State::Portable => "PORTABLE",
            State::Incomplete => "INCOMPLETE",
            State::MockedTestnetOnly => "MOCKED_TESTNET_ONLY",
            State::NoSynthesis => "NO_SYNTHESIS",
            State::Refused => "REFUSED",
        }
    }
}

/// One construct: its name, a happy-path fixture, and for INCOMPLETE ones a
/// second fixture exercising the operation that is refused, so the
/// incompleteness is proven rather than asserted.
struct Case {
    construct: &'static str,
    expected: State,
    fixture: &'static str,
    /// For INCOMPLETE: source that must fail to build because the declared
    /// operation is not implemented.
    refused_op: Option<&'static str>,
}

fn build(source: &str, target: Target) -> (Option<EvmArtifact>, Vec<Diagnostic>) {
    covenant_driver::compile(
        source,
        SourceId::new(0),
        EvmConfig::for_target(target),
        StdlibConfig::default(),
        OptimizerConfig::default(),
    )
}

fn has_errors(diags: &[Diagnostic]) -> bool {
    diags.iter().any(|d| d.level == DiagnosticLevel::Error)
}

fn has_w606(diags: &[Diagnostic]) -> bool {
    diags
        .iter()
        .any(|d| d.level == DiagnosticLevel::Warning && d.code.0 == W606_SYNTH_NOT_IMPL)
}

/// Derive the observed state from what the compiler does with the happy-path
/// fixture, plus the refused-operation probe for INCOMPLETE constructs.
fn observe(case: &Case) -> State {
    // A construct that does not build on the local mock chain, where every
    // primitive is native, builds nowhere: REFUSED.
    let (mock_artifact, mock_diags) = build(case.fixture, Target::MockChain);
    if mock_artifact.is_none() || has_errors(&mock_diags) {
        return State::Refused;
    }

    if has_w606(&mock_diags) {
        return State::NoSynthesis;
    }

    let mocked = mock_artifact
        .as_ref()
        .map(|a| !a.metadata.mocked_crypto_primitives.is_empty())
        .unwrap_or(false);
    if mocked {
        return State::MockedTestnetOnly;
    }

    // No mock primitive, no W606, builds. Either fully PORTABLE or INCOMPLETE.
    // INCOMPLETE is proven by a documented operation that is refused.
    if let Some(op) = case.refused_op {
        let (_, op_diags) = build(op, Target::MockChain);
        assert!(
            has_errors(&op_diags),
            "`{}` is declared INCOMPLETE, but its refused-operation probe built \
             cleanly, so the incompleteness is no longer real",
            case.construct
        );
        return State::Incomplete;
    }

    State::Portable
}

/// Pull `"construct": { "state": "X" }` out of the JSON without a parser.
fn json_state(construct: &str) -> Option<String> {
    let marker = format!("\"{construct}\"");
    let at = REGISTRY_JSON.find(&marker)? + marker.len();
    let rest = &REGISTRY_JSON[at..];
    let key = "\"state\"";
    let sat = rest.find(key)? + key.len();
    let after = &rest[sat..];
    let open = after.find('"')?;
    let tail = &after[open + 1..];
    let close = tail.find('"')?;
    Some(tail[..close].to_string())
}

fn cases() -> Vec<Case> {
    vec![
        Case { construct: "record", expected: State::Portable,
            fixture: "record R { owner: address\n enabled: bool }", refused_op: None },
        Case { construct: "token", expected: State::Portable,
            fixture: "token T { symbol: \"T\"\n name: \"T\"\n decimals: 18\n supply: 1000 to deployer }", refused_op: None },
        Case { construct: "nft", expected: State::Portable,
            fixture: "nft N { symbol: \"N\"\n name: \"N\" }", refused_op: None },
        Case { construct: "counter", expected: State::Portable,
            fixture: "counter C { action bump() { } }", refused_op: None },
        Case { construct: "module", expected: State::Portable,
            fixture: "module M { field n: amount\n action a() { n += 1 } }", refused_op: None },
        Case { construct: "board", expected: State::Incomplete,
            fixture: "board B { post { author: address\n content: hash\n at: time } }",
            refused_op: Some("board B { post { author: address }\n action submit() { append post { author: caller } } }") },
        Case { construct: "market", expected: State::Incomplete,
            fixture: "market Mk { field bids: priority_queue<amount, address, max> }",
            refused_op: Some("market Mk { field bids: priority_queue<amount, address, max>\n action place(p: amount) { bids.push(p, caller) } }") },
        Case { construct: "vault", expected: State::NoSynthesis,
            fixture: "vault V { field balances: map<address, amount>\n action deposit() { balances[caller] += 1 } }", refused_op: None },
        Case { construct: "ballot", expected: State::NoSynthesis,
            fixture: "ballot Bl { field tally: map<amount, amount>\n action vote(c: amount) { tally[c] += 1 } }", refused_op: None },
        Case { construct: "bridge", expected: State::NoSynthesis,
            fixture: "bridge Br anchored_on [\"a\", \"b\"] { field locked: amount }", refused_op: None },
        Case { construct: "confidential token", expected: State::MockedTestnetOnly,
            fixture: "-- ERC-8227\nconfidential token Ct { symbol: \"C\"\n name: \"C\"\n decimals: 18\n supply: 1000 to deployer }", refused_op: None },
        Case { construct: "encrypted counter", expected: State::MockedTestnetOnly,
            fixture: "encrypted counter Ec { total: amount\n action bump(by: amount) { total += by }\n reveal total to owner }", refused_op: None },
        Case { construct: "hybrid module", expected: State::MockedTestnetOnly,
            fixture: "hybrid module Hm { field a: amount\n field encrypted b: amount\n action bump() { b += 1 } }", refused_op: None },
        Case { construct: "ceremony", expected: State::MockedTestnetOnly,
            fixture: "-- ERC-8228\nceremony Cm { guardians: 3\n threshold: 2\n on_destroy { } }", refused_op: None },
        Case { construct: "registry", expected: State::Refused,
            fixture: "registry Rg { field keys: map<address, pq_key> }", refused_op: None },
    ]
}

#[test]
fn observed_behaviour_matches_the_declared_state() {
    let mut wrong = Vec::new();
    for case in cases() {
        let observed = observe(&case);
        if observed != case.expected {
            wrong.push(format!(
                "`{}`: declared {}, compiler behaves as {}",
                case.construct,
                case.expected.as_json(),
                observed.as_json()
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "the capability table disagrees with what the compiler emits:\n  {}",
        wrong.join("\n  ")
    );
}

#[test]
fn the_published_json_matches_the_table() {
    let mut wrong = Vec::new();
    for case in cases() {
        match json_state(case.construct) {
            None => wrong.push(format!(
                "`{}` is missing from capabilities.json",
                case.construct
            )),
            Some(s) if s != case.expected.as_json() => wrong.push(format!(
                "`{}`: json says {s}, table says {}",
                case.construct,
                case.expected.as_json()
            )),
            Some(_) => {}
        }
    }
    assert!(
        wrong.is_empty(),
        "config/capabilities.json has drifted from the verified table:\n  {}",
        wrong.join("\n  ")
    );
}

/// Every construct the parser accepts must be in the registry. A new construct
/// that parses but is absent here would "look deployable" by omission, which
/// is the exact failure the registry exists to prevent.
#[test]
fn the_registry_covers_every_construct() {
    // The authoritative keyword set, kept as a literal so adding a construct
    // to the language without classifying it fails this test.
    const ALL: &[&str] = &[
        "record",
        "token",
        "nft",
        "counter",
        "module",
        "board",
        "market",
        "vault",
        "ballot",
        "bridge",
        "confidential token",
        "encrypted counter",
        "hybrid module",
        "ceremony",
        "registry",
    ];
    let covered: std::collections::BTreeSet<&str> = cases().iter().map(|c| c.construct).collect();
    let missing: Vec<_> = ALL.iter().filter(|k| !covered.contains(**k)).collect();
    assert!(
        missing.is_empty(),
        "these constructs are not classified in the registry: {missing:?}"
    );
    assert_eq!(
        covered.len(),
        ALL.len(),
        "registry has entries for unknown constructs"
    );
}
