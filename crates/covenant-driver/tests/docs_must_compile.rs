//! Every Covenant program this repository publishes must compile.
//!
//! Nothing compiled the documentation, and it showed. The 15 tutorial pages
//! under `docs/examples` were generated in a V0.7-era dialect and shipped
//! stale at the public launch: 22 of 22 standalone contracts failed, and none
//! recovered from mechanical fixes. The site advertised an `interface`
//! construct that never existed in the lexer, and the fiction spread from the
//! manifesto into tutorial chapters and playground examples because no step
//! anywhere fed documented code to the compiler.
//!
//! This is that step. It collects every `.cov` under `examples/` and every
//! fenced `covenant` block in the repository's markdown, then compiles each
//! one. Code that a reader could copy has to work, and code that cannot work
//! has to say why, here, with the exact diagnostic it produces.
//!
//! The hard part of a docs gate is that a fragment illustrating one line is
//! not a defect. The rule used here is deliberately conservative: a block is
//! held to the standard only when it declares a top-level construct, which is
//! what makes it a compilation unit. Anything else is counted and skipped.
//! One consequence is worth stating, because it caught out an audit of this
//! very corpus: a chapter may split a single contract across two blocks, so
//! neither half compiles alone while the two together do. Splitting is a
//! presentation choice, not a defect, and a gate that cannot tell the
//! difference produces false alarms that cost more than the misses.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use covenant_diag::{Diagnostic, DiagnosticLevel, SourceId};
use covenant_evm_backend::{EvmConfig, Target};
use covenant_opt::OptimizerConfig;
use covenant_stdlib::StdlibConfig;

/// Constructs that open a compilation unit. A block containing one of these at
/// the start of a line is held to the compile standard.
const TOP_LEVEL: &[&str] = &[
    "record ",
    "token ",
    "nft ",
    "counter ",
    "module ",
    "board ",
    "market ",
    "vault ",
    "ballot ",
    "bridge ",
    "registry ",
    "ceremony ",
    "external contract ",
    "confidential token ",
    "encrypted counter ",
    "hybrid module ",
];

/// Programs that are published deliberately and cannot build, each pinned to
/// the exact error it must produce.
///
/// This is a ratchet, not an escape hatch. Pinning the code means the
/// exemption expires on its own: when dynamic `bytes` ABI encoding lands,
/// `registry` stops emitting E505, this table stops matching, and the test
/// says so instead of letting a now-buildable example sit excused forever.
const EXPECTED_REFUSALS: &[(&str, usize, u32)] = &[
    // A `ballot` audit fixture whose `only FirstTimeCaller` guard has no real
    // EVM authorization check, so the compiler refuses rather than emit a
    // guard that passes for every caller.
    ("examples/audit/03_ballot_open.cov", 0, 518),
    // `pq_key` is a dynamic `bytes` ABI type the backend cannot encode yet, so
    // every `registry` body is refused, including an empty one.
    ("examples/audit/05_registry_pq.cov", 0, 505),
    ("MILESTONES.md", 1, 505),
];

struct Unit {
    /// Repository-relative, forward-slashed.
    origin: String,
    /// Index of the fenced block within its file; 0 for a `.cov` file.
    index: usize,
    source: String,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/<crate> sits two levels below the repository root")
        .to_path_buf()
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

/// A block is a compilation unit when it declares a top-level construct with a
/// real name.
///
/// The name check is what keeps scaffolding out. The `/new` command docs show
/// shapes like `record <Name> {` and the MCP templates use `{{NAME}}`, which
/// are patterns a reader fills in, not programs. Requiring an identifier after
/// the keyword excludes them without needing an opt-out marker, and it fails
/// in the safe direction: a placeholder that looks like an identifier gets
/// compiled and complains, rather than a real contract being skipped.
fn is_unit(source: &str) -> bool {
    source.lines().any(|line| {
        TOP_LEVEL.iter().any(|keyword| {
            line.strip_prefix(keyword)
                .and_then(|rest| rest.chars().next())
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        })
    })
}

/// Pull ```covenant blocks out of markdown. Returns the body of each block.
fn covenant_fences(markdown: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = markdown;
    while let Some(at) = rest.find("```covenant") {
        let after = &rest[at + "```covenant".len()..];
        // Skip the rest of the info string.
        let Some(nl) = after.find('\n') else { break };
        let body = &after[nl + 1..];
        let Some(end) = body.find("```") else { break };
        out.push(body[..end].to_string());
        rest = &body[end + 3..];
    }
    out
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    const SKIP: &[&str] = &["target", ".git", "node_modules", "__pycache__", "dist"];
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if !SKIP.contains(&name.as_str()) {
                walk(&path, out);
            }
        } else {
            out.push(path);
        }
    }
}

fn collect() -> Vec<Unit> {
    let root = repo_root();
    let mut files = Vec::new();
    walk(&root, &mut files);
    files.sort();

    let mut units = Vec::new();
    for path in files {
        let origin = rel(&root, &path);
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        if origin.starts_with("examples/") && origin.ends_with(".cov") {
            units.push(Unit {
                origin,
                index: 0,
                source: text,
            });
        } else if origin.ends_with(".md") {
            for (index, source) in covenant_fences(&text).into_iter().enumerate() {
                units.push(Unit {
                    origin: origin.clone(),
                    index,
                    source,
                });
            }
        }
    }
    units
}

/// `None` when the unit builds; otherwise the first error code emitted.
fn build_failure(source: &str) -> Option<u32> {
    let (artifact, diagnostics): (_, Vec<Diagnostic>) = covenant_driver::compile(
        source,
        SourceId::new(0),
        EvmConfig::for_target(Target::MockChain),
        StdlibConfig::default(),
        OptimizerConfig::default(),
    );
    let first_error = diagnostics
        .iter()
        .find(|d| d.level == DiagnosticLevel::Error)
        .map(|d| d.code.0);
    match (artifact, first_error) {
        (Some(_), None) => None,
        (_, Some(code)) => Some(code),
        // No artifact and no error is still a failure, and a confusing one.
        (None, None) => Some(0),
    }
}

fn expected_refusal(origin: &str, index: usize) -> Option<u32> {
    EXPECTED_REFUSALS
        .iter()
        .find(|(p, i, _)| *p == origin && *i == index)
        .map(|(_, _, code)| *code)
}

#[test]
fn every_published_program_compiles_or_explains_itself() {
    let units = collect();
    let mut wrong = Vec::new();
    let mut compiled = 0usize;
    let mut refused = 0usize;
    let mut fragments = 0usize;

    for unit in &units {
        if !is_unit(&unit.source) {
            fragments += 1;
            continue;
        }
        let failure = build_failure(&unit.source);
        match (expected_refusal(&unit.origin, unit.index), failure) {
            (None, None) => compiled += 1,
            (None, Some(code)) => wrong.push(format!(
                "{}#{} does not build (E{code}), and is not a declared refusal. \
                 Fix the code, or pin it in EXPECTED_REFUSALS with its reason.",
                unit.origin, unit.index
            )),
            (Some(expected), Some(code)) if expected == code => refused += 1,
            (Some(expected), Some(code)) => wrong.push(format!(
                "{}#{} is pinned to fail with E{expected} but fails with E{code}. \
                 The reason it was excused has changed.",
                unit.origin, unit.index
            )),
            (Some(expected), None) => wrong.push(format!(
                "{}#{} is pinned to fail with E{expected} and now builds. \
                 Remove it from EXPECTED_REFUSALS.",
                unit.origin, unit.index
            )),
        }
    }

    assert!(
        wrong.is_empty(),
        "published Covenant code does not match what the compiler does:\n  {}\n\
         ({compiled} built, {refused} refused as declared, {fragments} fragments skipped)",
        wrong.join("\n  ")
    );

    // Meta-assertions. A gate that silently stops finding anything is worse
    // than no gate, because the green tick then certifies nothing.
    assert!(
        compiled >= 25,
        "only {compiled} programs were compiled, which is far below the corpus \
         size. The collector is probably matching nothing: check the fence \
         parser and the examples/ path."
    );
    assert_eq!(
        refused,
        EXPECTED_REFUSALS.len(),
        "{} refusals are declared but {refused} were observed, so at least one \
         pinned entry no longer matches a real file or block",
        EXPECTED_REFUSALS.len()
    );
    let files: BTreeSet<&str> = units.iter().map(|u| u.origin.as_str()).collect();
    assert!(
        files.len() >= 10,
        "only {} files contributed a program; the walk is not reaching the repository",
        files.len()
    );
}

/// The negative control, and the reason the test above is not vacuous.
///
/// If `build_failure` returned `None` for everything, every assertion would
/// pass while proving nothing. This pins both directions on code whose verdict
/// is known independently of any file on disk.
#[test]
fn the_compile_probe_distinguishes_good_from_bad() {
    assert_eq!(
        build_failure(
            "token T { symbol: \"T\"\n name: \"T\"\n decimals: 18\n supply: 1 to deployer }"
        ),
        None,
        "a plain token must build; if it does not, the probe is broken, not the docs"
    );
    assert!(
        build_failure("record R { owner: address\n action f() { nonexistent_thing() } }").is_some(),
        "a call to something undeclared must fail; the probe is not observing errors"
    );
}

/// The fence parser has to actually find blocks, and has to stop at the right
/// place. A parser that returned everything after the first fence would make
/// the corpus look like one giant broken program.
#[test]
fn the_fence_parser_reads_what_it_should() {
    let md = "text\n\n```covenant\nrecord A { }\n```\n\nmore\n\n```rust\nfn x() {}\n```\n\n\
              ```covenant title=demo\nrecord B { }\n```\n";
    let blocks = covenant_fences(md);
    assert_eq!(
        blocks.len(),
        2,
        "expected two covenant blocks, got {blocks:?}"
    );
    assert_eq!(blocks[0].trim(), "record A { }");
    assert_eq!(
        blocks[1].trim(),
        "record B { }",
        "an info string after the language must not swallow the first line"
    );
    assert!(
        !blocks.iter().any(|b| b.contains("fn x()")),
        "a rust block leaked into the covenant set"
    );
}
