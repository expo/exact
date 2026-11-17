//! Instructions kept decoded for the rest of one evaluation. A `Map` or
//! `Filter` callback's body runs once per item; from its first item on, each
//! instruction read is kept, within [`CODE`] and [`KEPT`], and not decoded
//! again for the next item.

use crate::machine::Instruction;

/// The longest code kept by either operand cache, in bytes: a plan is
/// untrusted, and each position table costs two bytes a byte.
/// euxy's activity log is 2,714.
pub(super) const CODE: usize = 1 << 16;

/// The most instructions either cache keeps; the rest decode as they are read.
/// The activity log keeps 703.
pub(super) const KEPT: usize = 4096;

/// What [`super::Run::fetch`] decoded, by position, once
/// [`Decoded::start`] has been called (a callback's first item begins).
#[derive(Default)]
pub(super) struct Decoded {
    len: usize,
    /// By byte position: 1 + the instruction's index in `list`, or 0; empty
    /// until an instruction needs keeping.
    at: Vec<u16>,
    /// Each decoded instruction and the position after it.
    list: Vec<(Instruction, usize)>,
}

impl Decoded {
    /// Enable caching for code no longer than [`CODE`]. Storage is allocated
    /// only when a fetch misses the shared pool.
    #[inline]
    pub(super) fn start(&mut self, len: usize) {
        if len <= CODE {
            self.len = len;
        }
    }

    /// The instruction at `pos` and the position after it, if kept.
    #[inline]
    pub(super) fn get(&self, pos: usize) -> Option<(Instruction, usize)> {
        match self.at.get(pos) {
            Some(&k) if k != 0 => self.list.get(k as usize - 1).copied(),
            _ => None,
        }
    }

    /// Keep the instruction at `pos`, once started and while fewer than
    /// [`KEPT`] are.
    #[inline]
    pub(super) fn keep(&mut self, pos: usize, ins: Instruction, next: usize) {
        if self.list.len() >= KEPT || pos >= self.len {
            return;
        }
        if self.at.is_empty() {
            // Exact: storage reused for a longer body would otherwise double.
            self.at.reserve_exact(self.len);
            self.at.resize(self.len, 0);
        }
        self.list.push((ins, next));
        self.at[pos] = self.list.len() as u16;
    }

    /// Empty, its storage kept for the next evaluation.
    pub(super) fn clear(&mut self) {
        self.len = 0;
        self.at.clear();
        self.list.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::run;
    use super::*;
    use crate::machine::Trap;
    use exact_plan::asm::Asm;
    use exact_plan::{Opcode, Value};

    #[test]
    fn callback_storage_is_lazy_bounded_and_cleared_between_evaluations() {
        let mut decoded = Decoded::default();
        decoded.start(CODE);
        assert_eq!(decoded.at.capacity(), 0);
        let ins = Instruction {
            op: Opcode::Return,
            pc: 0,
            args: [0; 3],
            number: 0.0,
        };
        for pos in 0..=KEPT {
            decoded.keep(pos, ins, pos + 1);
        }
        assert!(decoded.get(0).is_some());
        assert!(decoded.get(KEPT).is_none());
        assert_eq!(decoded.at.len(), CODE);
        assert_eq!(decoded.list.len(), KEPT);
        decoded.clear();
        for len in [0, CODE + 1] {
            decoded.start(len);
            decoded.keep(0, ins, 1);
            assert!(decoded.get(0).is_none());
            assert!(decoded.at.is_empty());
            assert!(decoded.list.is_empty());
        }
    }

    /// `map([1, 2, …, n], x => x == 3 ? none! : x * 10)`: an arm first taken
    /// at the third item, once the body is kept decoded, traps at its own pc.
    #[test]
    fn a_kept_callback_body_jumps_and_traps_where_it_did() {
        let body = |n: u32| {
            let mut a = Asm::new();
            let (end, scaled) = (a.label(), a.label());
            for i in 1..=n {
                a.number(f64::from(i));
            }
            a.list(n).each_item(Opcode::Map, end);
            a.load_local(0)
                .number(3.0)
                .simple(Opcode::Eq)
                .jump_if_false(scaled);
            let unwrap = a.simple(Opcode::None).position() as usize;
            a.simple(Opcode::Unwrap).place(scaled);
            a.load_local(0).number(10.0).simple(Opcode::Mul).place(end);
            (a, unwrap)
        };
        let (two, _) = body(2);
        let ten = |n: f64| Value::Number(n * 10.0);
        assert_eq!(
            run(two).unwrap().value,
            Value::list(vec![ten(1.0), ten(2.0)])
        );
        let (three, pc) = body(3);
        assert_eq!(run(three), Err(Trap::UnwrapNone { pc }));
    }
}
