//! Pass E: block-local SLoad coalescing.
//!
//! Successive `SLoad(f)` of the same field with no intervening write can
//! share a single load. Invalidation on any `SStore(f)`, `MapSet`,
//! `ListAppend`, or `ListSet` targeting the same field.

use std::collections::HashMap;

use covenant_ir::{GlobalId, IrFunction, Opcode, Value};

use crate::helpers::apply_replacements;

pub fn run_function(func: &mut IrFunction) -> bool {
    let mut replacements: HashMap<Value, Value> = HashMap::new();

    for block in &func.blocks {
        let mut cache: HashMap<GlobalId, Value> = HashMap::new();
        for instr in &block.instructions {
            match &instr.opcode {
                Opcode::SLoad(id) => {
                    if let Some(existing) = cache.get(id).copied() {
                        if let Some(result) = instr.result {
                            if result != existing {
                                replacements.insert(result, existing);
                            }
                        }
                    } else if let Some(result) = instr.result {
                        cache.insert(*id, result);
                    }
                }
                Opcode::SStore(id) => {
                    cache.remove(id);
                }
                Opcode::MapSet | Opcode::MapDelete | Opcode::ListAppend | Opcode::ListSet => {
                    // Conservative: invalidate the entire cache for any
                    // collection mutation: we don't track which field the
                    // mutated collection came from without more book-keeping.
                    cache.clear();
                }
                _ => {}
            }
        }
    }

    apply_replacements(func, &replacements)
}
