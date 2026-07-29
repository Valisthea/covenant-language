//! Pass C: Dead code elimination.
//!
//! Two sub-steps:
//! 1. **Unreachable-block removal.** BFS from entry following terminators.
//!    Any block not reached is deleted.
//! 2. **Pure-instruction elimination.** Ask the effect model (see
//!    [`crate::effects`]) which instructions must be kept whatever their
//!    result is worth, compute the live-value set as a fixpoint (seeded by
//!    terminator operands + the operands of those must-keep instructions),
//!    and remove the rest when their result isn't live.
//!
//! The two sub-steps read the SAME keep decision, and that is load-bearing.
//! Seeding liveness from a narrower set than the one used for retention is
//! how OMEGA V6 lost a `StructSet` address computation: the write survived,
//! the `ListGet` that computed the slot it wrote to did not. So the decision
//! is taken once, up front, and both halves index into it.

use std::collections::{HashSet, VecDeque};

use covenant_ir::{
    instr::{Instr, Terminator},
    BlockId, IrFunction, Value,
};

use crate::effects::{
    effect_of, integer_constants, trap_can_fire, trap_depends_only_on_operands, Effect,
};

pub fn run_function(func: &mut IrFunction) -> bool {
    let mut changed = false;

    // --- Unreachable-block removal ---
    let reachable = reachable_blocks(func);
    let total_blocks = func.blocks.len();
    if reachable.len() != total_blocks {
        // Retain only reachable blocks; renumber is NOT needed because
        // BlockId is an opaque key and other terminators already reference
        // only reachable blocks (by construction).
        func.blocks.retain(|b| reachable.contains(&b.id.0));
        changed = true;
    }

    // --- Pure-instruction elimination ---
    // Order matters: block removal first (so the indices below match the
    // final block list), then the keep decision, then liveness on top of it.
    let must_keep = decide_must_keep(func);
    let live = compute_live_set(func, &must_keep);

    for (bi, block) in func.blocks.iter_mut().enumerate() {
        let before = block.instructions.len();
        let kept: Vec<Instr> = std::mem::take(&mut block.instructions)
            .into_iter()
            .enumerate()
            .filter(|(ii, instr)| {
                must_keep[bi][*ii]
                    || match instr.result {
                        Some(v) => live.contains(&v),
                        None => true,
                    }
            })
            .map(|(_, instr)| instr)
            .collect();
        block.instructions = kept;
        if block.instructions.len() != before {
            changed = true;
        }
    }

    changed
}

/// Per-block, per-instruction: must this instruction survive regardless of
/// whether anything reads its result?
///
/// Trapping instructions get two refinements, both of which only ever say
/// "this one is safe to drop" when the trap provably cannot be observed:
///  - `trap_can_fire` settles the constant-operand cases exactly the way the
///    backend and the constant folder settle them;
///  - within one straight-line block, a repeat of an operand-determined trap
///    cannot fail where the first copy succeeded, so only the first copy has
///    to be kept for its trap. This is what keeps CSE useful on `bal - amt`
///    computed twice in a row.
fn decide_must_keep(func: &IrFunction) -> Vec<Vec<bool>> {
    let consts = integer_constants(func);
    func.blocks
        .iter()
        .map(|block| {
            // Keyed by opcode identity + operand values, the same fingerprint
            // CSE uses. Block-local only: across blocks there is no guarantee
            // the earlier copy ever executed.
            let mut trapped_already: HashSet<(String, Vec<Value>)> = HashSet::new();
            block
                .instructions
                .iter()
                .map(|instr| match effect_of(&instr.opcode) {
                    Effect::Pure => false,
                    Effect::State | Effect::Critical => true,
                    Effect::Trap => {
                        if !trap_can_fire(instr, &consts) {
                            return false;
                        }
                        if trap_depends_only_on_operands(&instr.opcode) {
                            let key = (format!("{:?}", instr.opcode), instr.operands.clone());
                            // `insert` is true for the FIRST occurrence only.
                            return trapped_already.insert(key);
                        }
                        true
                    }
                })
                .collect()
        })
        .collect()
}

fn reachable_blocks(func: &IrFunction) -> HashSet<u32> {
    let mut reached = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back(func.entry);
    while let Some(bid) = queue.pop_front() {
        if !reached.insert(bid.0) {
            continue;
        }
        let Some(block) = func.blocks.iter().find(|b| b.id == bid) else {
            continue;
        };
        // KSR-CVN-036: exhaustive match — adding a new Terminator variant
        // must force the compiler to flag this site so successors are not
        // silently dropped from the reachability walk.
        match &block.terminator {
            Terminator::Jump { target, .. } => queue.push_back(*target),
            Terminator::Branch {
                then_target,
                else_target,
                ..
            } => {
                queue.push_back(*then_target);
                queue.push_back(*else_target);
            }
            Terminator::FheBranch {
                then_target,
                else_target,
                merge_target,
                ..
            } => {
                queue.push_back(*then_target);
                queue.push_back(*else_target);
                queue.push_back(*merge_target);
            }
            Terminator::Return(_) | Terminator::Revert { .. } | Terminator::Unreachable => {}
        }
    }
    reached
}

fn compute_live_set(func: &IrFunction, must_keep: &[Vec<bool>]) -> HashSet<Value> {
    let mut live: HashSet<Value> = HashSet::new();
    let mut frontier: Vec<Value> = Vec::new();

    // Seed: terminator operands and the operands of every instruction the
    // effect model says survives on its own. A kept instruction still needs
    // its inputs: the underflow guard of `bal - amt` compares the SLoad of
    // `bal` against the parameter, so seeding from a narrower set than the
    // retention rule would keep the guard and delete the value it guards.
    for (bi, block) in func.blocks.iter().enumerate() {
        for (ii, instr) in block.instructions.iter().enumerate() {
            if must_keep[bi][ii] {
                for v in &instr.operands {
                    frontier.push(*v);
                }
            }
        }
        match &block.terminator {
            Terminator::Jump { args, .. } => frontier.extend(args.iter().copied()),
            Terminator::Branch {
                cond,
                then_args,
                else_args,
                ..
            } => {
                frontier.push(*cond);
                frontier.extend(then_args.iter().copied());
                frontier.extend(else_args.iter().copied());
            }
            Terminator::FheBranch {
                cond,
                then_args,
                else_args,
                ..
            } => {
                frontier.push(*cond);
                frontier.extend(then_args.iter().copied());
                frontier.extend(else_args.iter().copied());
            }
            Terminator::Return(Some(v)) => frontier.push(*v),
            Terminator::Return(None) | Terminator::Unreachable => {}
            Terminator::Revert { args, .. } => frontier.extend(args.iter().copied()),
        }
    }

    // Propagate liveness to defining-instruction operands.
    while let Some(v) = frontier.pop() {
        if !live.insert(v) {
            continue;
        }
        // If `v` was produced by an instruction, mark its operands live.
        for block in &func.blocks {
            for instr in &block.instructions {
                if instr.result == Some(v) {
                    for op in &instr.operands {
                        frontier.push(*op);
                    }
                }
            }
        }
    }

    // Function parameters and block parameters are always live (they're
    // bindings, not instruction results).
    for p in &func.params {
        live.insert(p.value);
    }
    for block in &func.blocks {
        for bp in &block.params {
            live.insert(*bp);
        }
    }
    live
}

/// Unused — kept for reachability helpers that may need parent info.
#[allow(dead_code)]
fn _bid_unused(_b: BlockId) {}

#[cfg(test)]
mod tests {
    //! KSR-CVN-026 / KSR-CVN-036 / OMEGA V3.6 F-19 regression tests.
    //!
    //! Build minimal `IrFunction`s by hand and verify DCE preserves
    //! verification opcodes, `FheBootstrap` and runtime traps even when their
    //! SSA results are unused. The end-to-end half of the F-19 regression
    //! lives in `tests/unit.rs`; what is here is the part source cannot
    //! isolate: which operand chain stays live, and where the block-local
    //! de-duplication of a trap stops.

    use std::collections::HashMap;

    use covenant_diag::{SourceId, Span};
    use covenant_ir::{
        block::IrBlock,
        function::{IrFunction, IrFunctionKind},
        id::{BlockId, FunctionId, GlobalId, Value},
        instr::{Instr, InstrMetadata, IrConstant, Terminator, ValueInfo},
        Opcode,
    };

    use super::run_function;

    fn span() -> Span {
        Span::new(SourceId::new(0), 0, 0)
    }

    fn empty_action() -> IrFunction {
        IrFunction {
            id: FunctionId(0),
            name: covenant_parser::ast::Ident {
                name: "t".into(),
                span: span(),
            },
            kind: IrFunctionKind::Action,
            params: vec![],
            returns: None,
            guards: vec![],
            qualifiers: vec![],
            annotations: vec![],
            blocks: vec![IrBlock {
                id: BlockId(0),
                params: vec![],
                instructions: vec![],
                terminator: Terminator::Return(None),
                span: span(),
            }],
            entry: BlockId(0),
            values: vec![],
            value_types: HashMap::new(),
            value_privacy: HashMap::new(),
            local_to_value: HashMap::new(),
            value_spans: HashMap::new(),
            span: span(),
        }
    }

    fn instr(opcode: Opcode, operands: Vec<Value>, result: Option<Value>) -> Instr {
        Instr {
            result,
            opcode,
            operands,
            metadata: InstrMetadata::default(),
            span: span(),
        }
    }

    fn count_op(f: &IrFunction, predicate: impl Fn(&Opcode) -> bool) -> usize {
        f.blocks
            .iter()
            .flat_map(|b| b.instructions.iter())
            .filter(|i| predicate(&i.opcode))
            .count()
    }

    /// Register `v` in the function's value table as the integer constant `n`,
    /// which is how the constant folder and the front end record literals.
    fn define_const(f: &mut IrFunction, v: Value, n: u128) {
        f.values.push((v, ValueInfo::Const(IrConstant::Integer(n))));
    }

    /// A two-block function: entry jumps to a second block. Used to show the
    /// trap de-duplication does NOT reach across a control-flow edge.
    fn two_block_action() -> IrFunction {
        let mut f = empty_action();
        f.blocks[0].terminator = Terminator::Jump {
            target: BlockId(1),
            args: vec![],
        };
        f.blocks.push(IrBlock {
            id: BlockId(1),
            params: vec![],
            instructions: vec![],
            terminator: Terminator::Return(None),
            span: span(),
        });
        f
    }

    #[test]
    fn ksr_026_dce_preserves_zk_verify_with_unused_result() {
        let mut f = empty_action();
        let proof = Value(0);
        let public_inputs = Value(1);
        let vk = Value(2);
        let res = Value(3); // unused
        f.blocks[0].instructions.push(instr(
            Opcode::ZkVerify,
            vec![proof, public_inputs, vk],
            Some(res),
        ));
        run_function(&mut f);
        assert_eq!(
            count_op(&f, |o| matches!(o, Opcode::ZkVerify)),
            1,
            "ZkVerify must survive DCE even when its result is dead"
        );
    }

    #[test]
    fn ksr_026_dce_preserves_pq_verify_with_unused_result() {
        let mut f = empty_action();
        let msg = Value(0);
        let sig = Value(1);
        let pk = Value(2);
        let res = Value(3);
        f.blocks[0].instructions.push(instr(
            Opcode::PqVerifyDilithium,
            vec![msg, sig, pk],
            Some(res),
        ));
        run_function(&mut f);
        assert_eq!(
            count_op(&f, |o| matches!(o, Opcode::PqVerifyDilithium)),
            1,
            "PqVerifyDilithium must survive DCE"
        );
    }

    #[test]
    fn ksr_026_dce_preserves_pq_hybrid_verify() {
        let mut f = empty_action();
        let msg = Value(0);
        let sig = Value(1);
        let pk = Value(2);
        let res = Value(3);
        f.blocks[0]
            .instructions
            .push(instr(Opcode::PqHybridVerify, vec![msg, sig, pk], Some(res)));
        run_function(&mut f);
        assert_eq!(count_op(&f, |o| matches!(o, Opcode::PqHybridVerify)), 1);
    }

    #[test]
    fn ksr_026_dce_preserves_vdf_verify() {
        let mut f = empty_action();
        let proof = Value(0);
        let input = Value(1);
        let delay = Value(2);
        let session = Value(3);
        let res = Value(4);
        f.blocks[0].instructions.push(instr(
            Opcode::VdfVerify,
            vec![proof, input, delay, session],
            Some(res),
        ));
        run_function(&mut f);
        assert_eq!(count_op(&f, |o| matches!(o, Opcode::VdfVerify)), 1);
    }

    #[test]
    fn ksr_026_dce_preserves_fhe_bootstrap() {
        let mut f = empty_action();
        let ct = Value(0);
        let refreshed = Value(1); // unused — DCE would normally drop
        f.blocks[0]
            .instructions
            .push(instr(Opcode::FheBootstrap, vec![ct], Some(refreshed)));
        run_function(&mut f);
        assert_eq!(
            count_op(&f, |o| matches!(o, Opcode::FheBootstrap)),
            1,
            "FheBootstrap must survive DCE — removing it silently busts noise budget"
        );
    }

    #[test]
    fn ksr_026_dce_still_drops_pure_unused_add() {
        // Sanity: pure ops with unused results are still removed.
        let mut f = empty_action();
        let a = Value(0);
        let b = Value(1);
        let r = Value(2);
        f.blocks[0]
            .instructions
            .push(instr(Opcode::Add, vec![a, b], Some(r)));
        run_function(&mut f);
        assert_eq!(
            count_op(&f, |o| matches!(o, Opcode::Add)),
            0,
            "pure Add with unused result must still be DCE'd"
        );
    }

    // ---------------- OMEGA V3.6 F-19 ----------------

    #[test]
    fn f19_every_trapping_arithmetic_opcode_survives_a_dead_result() {
        // One sweep over the whole family named in the finding. Each is built
        // with runtime (non-constant) operands so no static refinement can
        // discharge the guard.
        for op in [
            Opcode::Div,
            Opcode::Mod,
            Opcode::AddChecked,
            Opcode::SubChecked,
            Opcode::MulChecked,
        ] {
            let mut f = empty_action();
            let a = Value(0);
            let b = Value(1);
            let dead = Value(2);
            f.blocks[0]
                .instructions
                .push(instr(op.clone(), vec![a, b], Some(dead)));
            run_function(&mut f);
            assert_eq!(
                count_op(&f, |o| *o == op),
                1,
                "{op:?} carries a runtime revert and must survive a dead result"
            );
        }
    }

    #[test]
    fn f19_a_retained_trap_keeps_its_operand_chain_live() {
        // The half that bit OMEGA V6 on `StructSet`: retention and liveness
        // seeding have to agree. The underflow guard of `bal - amt` compares
        // the SLoad of `bal`, so dropping the SLoad would ship a guard
        // reading a memory slot nothing ever wrote.
        let mut f = empty_action();
        let bal = Value(0);
        let amt = Value(1);
        let dead = Value(2);
        f.blocks[0]
            .instructions
            .push(instr(Opcode::SLoad(GlobalId(0)), vec![], Some(bal)));
        f.blocks[0]
            .instructions
            .push(instr(Opcode::SubChecked, vec![bal, amt], Some(dead)));
        run_function(&mut f);
        assert_eq!(
            count_op(&f, |o| matches!(o, Opcode::SubChecked)),
            1,
            "the checked subtraction must survive"
        );
        assert_eq!(
            count_op(&f, |o| matches!(o, Opcode::SLoad(_))),
            1,
            "the SLoad feeding a retained guard must be seeded live by it"
        );
    }

    #[test]
    fn f19_literal_zero_divisor_reaches_the_backend() {
        // E519 is a codegen diagnostic: it only fires if the instruction is
        // still there to be lowered.
        let mut f = empty_action();
        let lhs = Value(0);
        let zero = Value(1);
        let dead = Value(2);
        define_const(&mut f, zero, 0);
        f.blocks[0]
            .instructions
            .push(instr(Opcode::Div, vec![lhs, zero], Some(dead)));
        run_function(&mut f);
        assert_eq!(
            count_op(&f, |o| matches!(o, Opcode::Div)),
            1,
            "division by a literal zero must reach codegen so E519 can refuse it"
        );
    }

    #[test]
    fn f19_literal_non_zero_divisor_is_still_eliminated() {
        // Counterpart: the backend emits no guard here, so there is no trap
        // to preserve. Keeping it would tax every `value * bps / 10000`.
        let mut f = empty_action();
        let lhs = Value(0);
        let ten_k = Value(1);
        let dead = Value(2);
        define_const(&mut f, ten_k, 10_000);
        f.blocks[0]
            .instructions
            .push(instr(Opcode::Div, vec![lhs, ten_k], Some(dead)));
        run_function(&mut f);
        assert_eq!(
            count_op(&f, |o| matches!(o, Opcode::Div)),
            0,
            "a provably non-zero divisor emits no guard, so the dead Div goes"
        );
    }

    #[test]
    fn f19_checked_arithmetic_on_safe_constants_is_still_eliminated() {
        // What the constant folder already proved cannot overflow `u128`
        // cannot overflow a 256-bit word either, so the guard is unreachable
        // and `1 + 2 * 3` still collapses to one constant.
        let mut f = empty_action();
        let two = Value(0);
        let three = Value(1);
        let dead = Value(2);
        define_const(&mut f, two, 2);
        define_const(&mut f, three, 3);
        f.blocks[0]
            .instructions
            .push(instr(Opcode::MulChecked, vec![two, three], Some(dead)));
        run_function(&mut f);
        assert_eq!(count_op(&f, |o| matches!(o, Opcode::MulChecked)), 0);
    }

    #[test]
    fn f19_checked_arithmetic_that_would_overflow_is_kept() {
        // The folder refused to fold this one, precisely because it traps.
        let mut f = empty_action();
        let small = Value(0);
        let big = Value(1);
        let dead = Value(2);
        define_const(&mut f, small, 1);
        define_const(&mut f, big, 5);
        f.blocks[0]
            .instructions
            .push(instr(Opcode::SubChecked, vec![small, big], Some(dead)));
        run_function(&mut f);
        assert_eq!(
            count_op(&f, |o| matches!(o, Opcode::SubChecked)),
            1,
            "1 - 5 underflows: the revert is the whole point of the instruction"
        );
    }

    #[test]
    fn f19_repeat_of_an_operand_determined_trap_in_one_block_is_dropped() {
        // Straight-line code, same SSA operands: the second copy cannot fail
        // where the first succeeded, so only the first has to be kept. This
        // is what stops the fix from cancelling CSE.
        let mut f = empty_action();
        let a = Value(0);
        let b = Value(1);
        f.blocks[0]
            .instructions
            .push(instr(Opcode::SubChecked, vec![a, b], Some(Value(2))));
        f.blocks[0]
            .instructions
            .push(instr(Opcode::SubChecked, vec![a, b], Some(Value(3))));
        run_function(&mut f);
        assert_eq!(
            count_op(&f, |o| matches!(o, Opcode::SubChecked)),
            1,
            "the redundant copy of a trap already taken is not a second trap"
        );
    }

    #[test]
    fn f19_traps_with_different_operands_are_both_kept() {
        let mut f = empty_action();
        let a = Value(0);
        let b = Value(1);
        let c = Value(2);
        f.blocks[0]
            .instructions
            .push(instr(Opcode::SubChecked, vec![a, b], Some(Value(3))));
        f.blocks[0]
            .instructions
            .push(instr(Opcode::SubChecked, vec![a, c], Some(Value(4))));
        run_function(&mut f);
        assert_eq!(
            count_op(&f, |o| matches!(o, Opcode::SubChecked)),
            2,
            "different operands are different failure conditions"
        );
    }

    #[test]
    fn f19_trap_dedup_does_not_cross_a_block_boundary() {
        // De-duplication is block-local because nothing guarantees the
        // earlier block executed on the path that reaches the later one.
        let mut f = two_block_action();
        let a = Value(0);
        let b = Value(1);
        f.blocks[0]
            .instructions
            .push(instr(Opcode::Div, vec![a, b], Some(Value(2))));
        f.blocks[1]
            .instructions
            .push(instr(Opcode::Div, vec![a, b], Some(Value(3))));
        run_function(&mut f);
        assert_eq!(
            count_op(&f, |o| matches!(o, Opcode::Div)),
            2,
            "a trap in another block may never have executed"
        );
    }

    #[test]
    fn f19_unlowered_opcodes_keep_their_revert_stub() {
        // KSR-CVN-022 and OMEGA V6 CRT-004 answer an unimplementable opcode
        // with a hard diagnostic plus a jump to `__revert__`. Both halves
        // need the instruction to reach codegen: delete it and the error
        // never fires and a defeated access-control guard ships silently.
        for op in [
            Opcode::PqTopKey,
            Opcode::PqLength,
            Opcode::ShamirReconstruct,
            Opcode::CallerMatchesPrincipal,
        ] {
            let mut f = empty_action();
            let a = Value(0);
            f.blocks[0]
                .instructions
                .push(instr(op.clone(), vec![a], Some(Value(1))));
            run_function(&mut f);
            assert_eq!(
                count_op(&f, |o| *o == op),
                1,
                "{op:?} lowers to a revert stub that must not be optimized away"
            );
        }
    }

    #[test]
    fn f19_dead_list_read_keeps_its_bounds_trap() {
        let mut f = empty_action();
        let list = Value(0);
        let idx = Value(1);
        f.blocks[0]
            .instructions
            .push(instr(Opcode::ListGet, vec![list, idx], Some(Value(2))));
        run_function(&mut f);
        assert_eq!(count_op(&f, |o| matches!(o, Opcode::ListGet)), 1);
    }

    #[test]
    fn f19_the_pass_is_still_a_pass() {
        // Guard against the opposite failure mode. An effect model that
        // keeps everything is not a model, it is a disabled optimizer, and
        // it would be just as invisible in the trap tests above.
        let mut f = empty_action();
        let a = Value(0);
        let b = Value(1);
        f.blocks[0]
            .instructions
            .push(instr(Opcode::SLoad(GlobalId(0)), vec![], Some(Value(2))));
        f.blocks[0]
            .instructions
            .push(instr(Opcode::MapGet, vec![a, b], Some(Value(3))));
        f.blocks[0]
            .instructions
            .push(instr(Opcode::Keccak, vec![a], Some(Value(4))));
        f.blocks[0]
            .instructions
            .push(instr(Opcode::Add, vec![a, b], Some(Value(5))));
        run_function(&mut f);
        assert_eq!(
            f.blocks[0].instructions.len(),
            0,
            "dead pure reads and pure arithmetic must all still be eliminated"
        );
    }
}
