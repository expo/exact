//! Opcode operands decoded once for an immutable plan. Positions and jumps
//! remain byte offsets; a fetch outside a kept instruction uses the decoder.
use super::{
    decode,
    decoded::{CODE, KEPT},
    Instruction,
};
use exact_plan::{bytes::Reader, Plan};

#[derive(Default)]
pub(crate) struct Constants {
    at: Box<[u16]>,
    list: Vec<(Instruction, usize)>,
}

impl Constants {
    pub(crate) fn new(plan: &Plan) -> Self {
        let code = &plan.code;
        if code.len() > CODE {
            return Self::default();
        }
        let mut pool = Self {
            at: vec![0; code.len()].into_boxed_slice(),
            list: Vec::new(),
        };
        let mut cursor = Reader::new(code);
        while !cursor.is_empty() && pool.list.len() < KEPT {
            let pos = cursor.position();
            match decode(&mut cursor) {
                Ok(ins) => {
                    pool.list.push((ins, cursor.position() - pos));
                    pool.at[pos] = pool.list.len() as u16;
                }
                Err(_) => {
                    // Unreachable bytes must not refuse another body. A fetch
                    // that reaches them still decodes and traps at its own pc.
                    cursor = Reader::new(code);
                    let _ = cursor.bytes(pos + 1);
                }
            }
        }
        pool
    }

    pub(super) fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub(super) fn get(&self, base: usize, pos: usize, len: usize) -> Option<(Instruction, usize)> {
        let index = self.at.get(base.checked_add(pos)?)?.checked_sub(1)?;
        let (mut ins, width) = *self.list.get(index as usize)?;
        let next = pos.checked_add(width)?;
        if next > len {
            return None;
        }
        ins.pc = pos;
        Some((ins, next))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine::Host;
    use crate::vm::{self, Env, Strings};
    use exact_plan::{asm::Asm, builder::PlanBuilder, Opcode, Value};
    use std::sync::Arc;

    fn env<'a>(plan: &'a Plan, strings: &'a Strings) -> Env<'a> {
        Env {
            plan,
            strings,
            router: None,
            lists: None,
            format: None,
            geometry: None,
            slots: &[],
            derives: &[],
            resources: (&[]).into(),
            params: &[],
            frames: &[],
            now_ms: 0.0,
            pending_resources: (&[]).into(),
            current_resources: (&[]).into(),
            failed_resources: (&[]).into(),
            failed_why: (&[]).into(),
            pending_mutations: &[],
            failed_mutations: &[],
            store_dependent_derives: &[],
            store_dependent_resources: (&[]).into(),
        }
    }

    #[test]
    fn oversized_code_does_not_allocate_a_shared_operand_cache() {
        let mut builder = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
        let mut body = Asm::new();
        body.number(42.0);
        let code = builder.code(body);
        let mut plan = builder.finish().unwrap();
        plan.code
            .to_mut()
            .resize((1 << 16) + 1, Opcode::Return as u8);
        let plan = plan.redecode().unwrap();
        let pool = Arc::new(Constants::new(&plan));
        assert!(pool.at.is_empty());
        assert_eq!(pool.list.capacity(), 0);
        let strings = Strings::with_constants(&plan, pool);
        assert_eq!(
            vm::eval(plan.code(code), &env(&plan, &strings), &[])
                .unwrap()
                .value,
            Value::Number(42.0)
        );
    }

    #[test]
    fn the_shared_instruction_limit_leaves_the_tail_to_the_decoder() {
        let mut builder = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
        let mut padding = Asm::new();
        for _ in 0..4096 {
            padding.simple(Opcode::Return);
        }
        builder.code(padding);
        let mut body = Asm::new();
        let end = body.label();
        body.number(1.0)
            .number(2.0)
            .list(2)
            .each_item(Opcode::Map, end);
        let callback = body.position() as usize;
        body.load_local(0)
            .number(10.0)
            .simple(Opcode::Mul)
            .place(end);
        let code = builder.code(body);
        let mut bad = Asm::new();
        bad.simple(Opcode::None).simple(Opcode::Unwrap);
        let bad = builder.code(bad);
        let plan = builder.finish().unwrap();
        let pool = Arc::new(Constants::new(&plan));
        assert!(pool.at.len() <= 1 << 16);
        assert!(pool.list.len() <= 4096);
        assert!(pool.list.capacity() <= 4096);
        assert!(pool
            .get(code.offset as usize, 0, code.len as usize)
            .is_none());
        let strings = Strings::with_constants(&plan, Arc::clone(&pool));
        let environment = env(&plan, &strings);
        let mut run = vm::Run {
            env: &environment,
            code: plan.code(code),
            allowed_writes: &[],
            cursor: Reader::new(plan.code(code)),
            constants: Some((&pool, code.offset as usize)),
            out: vm::Outcome::default(),
            extents: vm::Extents::default(),
            decoded: vm::Decoded::default(),
            steps: 0,
            calls: 0,
            depth: 0,
        };
        assert_eq!(
            crate::machine::run(&mut run, &mut vm::Machine::new()).unwrap(),
            Value::list(vec![Value::Number(10.0), Value::Number(20.0)])
        );
        assert!(run.decoded.get(callback).is_some());
        let cursor = run.cursor.position();
        run.fetch(callback).unwrap().unwrap();
        assert_eq!(
            run.cursor.position(),
            cursor,
            "the callback is not decoded again"
        );
        let plain = Strings::new(&plan);
        let mut scratch = vm::Scratch::default();
        for _ in 0..3 {
            for code in [code, bad] {
                let bytes = plan.code(code);
                let cached = vm::eval_reusing(bytes, &env(&plan, &strings), &[], &mut scratch);
                let raw = vm::eval(bytes, &env(&plan, &plain), &[]);
                assert_eq!(format!("{cached:?}"), format!("{raw:?}"));
            }
        }
        assert_eq!(
            vm::eval(plan.code(code), &env(&plan, &strings), &[])
                .unwrap()
                .value,
            Value::list(vec![Value::Number(10.0), Value::Number(20.0)])
        );
        assert_eq!(
            vm::eval(plan.code(bad), &env(&plan, &strings), &[]),
            Err(vm::Trap::UnwrapNone { pc: 1 })
        );
    }

    #[test]
    fn pooled_operands_preserve_offsets_traps_and_evaluation_state() {
        let mut builder = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
        let mut prefix = Asm::new();
        prefix.number(99.0);
        builder.code(prefix);
        let mut body = Asm::new();
        let end = body.label();
        body.number(1.0)
            .number(2.0)
            .number(3.0)
            .list(3)
            .each_item(Opcode::Map, end)
            .load_local(0)
            .number(10.0)
            .simple(Opcode::Mul)
            .place(end)
            .simple(Opcode::Return);
        let code = builder.code(body);
        let mut bad = Asm::new();
        bad.simple(Opcode::None).simple(Opcode::Unwrap);
        let bad = builder.code(bad);
        let plan = builder.finish().unwrap();
        let pool = Arc::new(Constants::new(&plan));
        let strings = Strings::with_constants(&plan, Arc::clone(&pool));
        let plain = Strings::new(&plan);
        let env = env(&plan, &strings);
        let mut scratch = vm::Scratch::default();
        for _ in 0..3 {
            for code in [code, bad] {
                let bytes = plan.code(code);
                let cached = vm::eval_reusing(bytes, &env, &[], &mut scratch);
                let raw = vm::eval(
                    bytes,
                    &Env {
                        strings: &plain,
                        ..env
                    },
                    &[],
                );
                assert_eq!(format!("{cached:?}"), format!("{raw:?}"));
            }
        }
        assert_eq!(
            vm::eval(plan.code(bad), &env, &[]),
            Err(vm::Trap::UnwrapNone { pc: 1 })
        );
        // Also fetch truncated bodies and instruction operands entered by a jump.
        for base in 0..plan.code.len() {
            for len in 1..=plan.code.len() - base {
                let bytes = &plan.code[base..base + len];
                for pos in 0..len {
                    if let Some(kept) = pool.get(base, pos, len) {
                        let mut cursor = Reader::new(bytes);
                        cursor.bytes(pos).unwrap();
                        let raw = decode(&mut cursor).unwrap();
                        assert_eq!(
                            format!("{kept:?}"),
                            format!("{:?}", (raw, cursor.position()))
                        );
                    }
                }
            }
        }
        assert_eq!(
            vm::eval(plan.code(code), &env, &[]).unwrap().value,
            Value::list(vec![
                Value::Number(10.0),
                Value::Number(20.0),
                Value::Number(30.0)
            ])
        );
    }
}
