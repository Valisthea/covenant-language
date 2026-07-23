//! Smoke test: build IR for all 5 Basics fixtures.

use std::collections::BTreeMap;

use covenant_diag::SourceId;
use covenant_ir::{build_ir, validate};
use covenant_lexer::tokenize;
use covenant_parser::parse;
use covenant_privacy::analyze_privacy;
use covenant_resolver::resolve;
use covenant_types::typecheck;

fn opcode_family(op: &covenant_ir::Opcode) -> &'static str {
    use covenant_ir::Opcode::*;
    match op {
        Add | Sub | Mul | Div | Mod | SignedNeg | BitAnd | BitOr | BitXor | ShiftLeft
        | ShiftRight | BitNot | AddChecked | SubChecked | MulChecked | Eq | Ne | Lt | Le | Gt
        | Ge | LogicalAnd | LogicalOr | LogicalNot | TimeAdd | TimeSub | DurationAdd
        | DurationSub | DurationScale => "arith",
        SLoad(_) | SStore(_) | MLoad | MStore => "storage",
        MapGet | MapSet | MapDelete | MapHas | MapLength | MapKeys | MapValues | ListAppend
        | ListGet | ListSet | ListLength | ListSlice | ListFirst | ListLast | ListArgMax
        | ListArgMin | PqInsert | PqPop | PqTopKey | PqTopValue | PqLength => "collection",
        StructNew(_) | StructGet(_) | StructSet(_) | ChoiceMatch(_) | TextConcat | BytesConcat
        | ListConcat | BytesLength | TextLength | TextEquals => "data",
        Keccak | Blake2 | Hmac | AbiEncode | AbiDecode | AbiPack => "crypto",
        FheEncryptTrivial | FheEncryptFresh | FheAdd | FheSub | FheMul | FheCmpEq | FheCmpNe
        | FheCmpLt | FheCmpLe | FheCmpGt | FheCmpGe | FheAnd | FheOr | FheNot | FheSelect
        | FheBootstrap | FheCiphertextHash => "fhe",
        ZkVerify | ZkNullifier | ZkProofPayload | VdfEval | VdfVerify => "zk",
        PqVerifyDilithium | PqHybridVerify | PqRand | KyberEncrypt | KyberDecrypt => "pq",
        AmnesiaBegin | AmnesiaSubmitShare | AmnesiaFinalize | ShamirSplit | ShamirReconstruct
        | VdfLock | VdfUnlock | DestructionProof => "amnesia",
        Emit(_) | Transfer => "emit",
        IsCallerSender | CallerMatchesPrincipal | BuiltinPredicateCall(_) => "auth",
        LoadCaller | LoadNow | LoadBlockNumber | LoadBlockTimestamp | LoadMsgValue
        | LoadMsgSender | LoadDeployer | LoadThis | LoadChainId | LoadZeroAddress => "lang",
        RevealDecrypt => "reveal",
        Assert | AssertEncrypted => "assert",
        Coerce(_) => "coerce",
        ExternalCall { .. } => "external",
    }
}

fn main() {
    for (name, src) in [
        (
            "example_01_hello",
            include_str!("../../covenant-lexer/tests/fixtures/example_01_hello.cov"),
        ),
        (
            "example_02_coin",
            include_str!("../../covenant-lexer/tests/fixtures/example_02_coin.cov"),
        ),
        (
            "example_03_open_ballot",
            include_str!("../../covenant-lexer/tests/fixtures/example_03_open_ballot.cov"),
        ),
        (
            "example_04_shielded_counter",
            include_str!("../../covenant-lexer/tests/fixtures/example_04_shielded_counter.cov"),
        ),
        (
            "example_05_quantum_board",
            include_str!("../../covenant-lexer/tests/fixtures/example_05_quantum_board.cov"),
        ),
    ] {
        let (toks, _) = tokenize(src, SourceId::new(0));
        let (file, _) = parse(&toks, SourceId::new(0));
        let (res, _) = resolve(file.unwrap(), SourceId::new(0));
        let (typed, _) = typecheck(res, SourceId::new(0));
        let (checked, _) = analyze_privacy(typed, SourceId::new(0));
        let (module, diags) = build_ir(checked, SourceId::new(0));

        let mut total_blocks = 0usize;
        let mut total_instrs = 0usize;
        let mut by_family: BTreeMap<&'static str, usize> = BTreeMap::new();
        for f in &module.functions {
            total_blocks += f.blocks.len();
            for b in &f.blocks {
                total_instrs += b.instructions.len();
                for i in &b.instructions {
                    *by_family.entry(opcode_family(&i.opcode)).or_default() += 1;
                }
            }
        }

        println!(
            "{name:30}  fns:{:2} blocks:{:2} instrs:{:3} by_family:{:?}  diags:{}",
            module.functions.len(),
            total_blocks,
            total_instrs,
            by_family,
            diags.len()
        );

        let vdiags = validate(&module);
        if !vdiags.is_empty() {
            println!("    validator: {} issues", vdiags.len());
            for d in &vdiags {
                println!(
                    "      - {:?} @ {}..{}: {}",
                    d.code, d.span.start, d.span.end, d.message
                );
            }
        }
    }
}
