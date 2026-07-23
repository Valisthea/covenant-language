//! Pass C: Dead code elimination.
//!
//! Two sub-steps:
//! 1. **Unreachable-block removal.** BFS from entry following terminators.
//!    Any block not reached is deleted.
//! 2. **Pure-instruction elimination.** Compute the live-value set as a
//!    fixpoint (seeded by terminator operands + operands of side-effecting
//!    instructions). Remove instructions whose result isn't live AND whose
//!    opcode is pure.

use std::collections::{HashSet, VecDeque};

use covenant_ir::{instr::Terminator, BlockId, IrFunction, Opcode, Value};

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
    let live = compute_live_set(func);

    for block in &mut func.blocks {
        let before = block.instructions.len();
        block.instructions.retain(|instr| {
            if is_side_effecting(&instr.opcode) {
                return true;
            }
            match instr.result {
                Some(v) => live.contains(&v),
                None => true,
            }
        });
        if block.instructions.len() != before {
            changed = true;
        }
    }

    changed
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

fn compute_live_set(func: &IrFunction) -> HashSet<Value> {
    let mut live: HashSet<Value> = HashSet::new();
    let mut frontier: Vec<Value> = Vec::new();

    // Seed: terminator operands and side-effecting instruction operands.
    for block in &func.blocks {
        for instr in &block.instructions {
            if is_side_effecting(&instr.opcode) {
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

fn is_side_effecting(opcode: &Opcode) -> bool {
    use Opcode::*;
    // KSR-CVN-026: also preserve verify ops + FheBootstrap (noise refresh)
    // even when their SSA result is unused. Removing a `ZkVerify` because
    // its boolean result was constant-folded into a dead branch would
    // silently strip the verification.
    if opcode.is_verify_or_noise_critical() {
        return true;
    }
    matches!(
        opcode,
        SStore(_)
            | MStore
            | MapSet
            | MapDelete
            | ListAppend
            | ListSet
            // OMEGA V6 (CRT-003/MED-001 follow-on): StructSet performs a real
            // SSTORE (via codegen's address-relative write) and was missing
            // here. Because compute_live_set only SEEDS the live-value
            // frontier from side-effecting instructions' operands, an
            // opcode missing from this list doesn't just risk being removed
            // itself (void-result instructions are always kept, see
            // run_function above) -- it silently fails to mark ITS OWN
            // operands live, so whatever computed the storage ADDRESS it
            // writes to (a `ListGet` on a struct-element list, a pure/
            // non-side-effecting instruction) gets treated as dead code and
            // deleted. The address then reads back as memory's default
            // zero at runtime, and `list[idx].field = value` silently
            // corrupts an unrelated slot instead of writing the field.
            | StructSet(_)
            | Emit(_)
            | Transfer
            | Assert
            | AssertEncrypted
            | AmnesiaBegin
            | AmnesiaSubmitShare
            | AmnesiaFinalize
            | DestructionProof
            | VdfLock
            | VdfUnlock
            | PqInsert
            | PqPop
            | RevealDecrypt
            | ExternalCall { .. }
    )
}

/// Unused — kept for reachability helpers that may need parent info.
#[allow(dead_code)]
fn _bid_unused(_b: BlockId) {}

#[cfg(test)]
mod tests {
    //! KSR-CVN-026 / KSR-CVN-036 regression tests.
    //!
    //! Build minimal `IrFunction`s by hand and verify DCE preserves
    //! verification opcodes and `FheBootstrap` even when their SSA results
    //! are unused.

    use std::collections::HashMap;

    use covenant_diag::{SourceId, Span};
    use covenant_ir::{
        block::IrBlock,
        function::{IrFunction, IrFunctionKind},
        id::{BlockId, FunctionId, Value},
        instr::{Instr, InstrMetadata, Terminator},
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
}
