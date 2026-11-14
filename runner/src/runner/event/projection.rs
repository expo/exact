use super::*;

impl<D: DataSource> Runner<D> {
    pub(super) fn binding_projection(
        &self,
        code: Code,
        frames: &[crate::vm::Frame],
        string_bytes: &mut usize,
    ) -> Result<BindingScalar, ActionBindingRefusal> {
        let value = self.projection_value(self.plan.code(code), frames, &mut 0, &mut 0)?;
        BindingScalar::capture(&value, string_bytes)
    }

    fn projection_value(
        &self,
        code: &[u8],
        frames: &[crate::vm::Frame],
        bytes: &mut usize,
        fields: &mut usize,
    ) -> Result<Value, ActionBindingRefusal> {
        use ActionBindingRefusal::{Limit, Unsupported};
        let (code, frames) = self
            .plan
            .read_code_in(code, frames, |f| f.region)
            .ok_or(Unsupported)?;
        *bytes += code.len();
        if *bytes > 128 {
            return Err(Limit);
        }
        let mut r = crate::vm::instructions(code).peekable();
        let first = binding_step(&mut r)?;
        let op = first.op;
        let mut value = match op {
            Opcode::Number => Value::Number(first.number),
            Opcode::Bool => Value::Bool(first.args[0] != 0),
            Opcode::Unit => Value::Unit,
            Opcode::Str => {
                let text = self.plan.str(exact_plan::StrId(first.args[0] as u32));
                if text.len() > BINDING_STRING {
                    return Err(Limit);
                }
                Value::str(text)
            }
            Opcode::LoadDerive => {
                let row = self
                    .plan
                    .unsettled_body(first.args[0] as u32)
                    .ok_or(Unsupported)?;
                let frames = self
                    .plan
                    .owner_frames(frames, row.owner, |f| f.region)
                    .ok_or(Unsupported)?;
                self.projection_value(self.plan.code(row.body), frames, bytes, fields)?
            }
            Opcode::LoadSlot => {
                let id = first.args[0] as u32;
                let row = self.plan.slots.get(id as usize).ok_or(Unsupported)?;
                match self.plan.owner_region(row) {
                    Some(owner) => crate::vm::Frame::row_of(frames, owner.0)
                        .and_then(|slots| slots.borrow().get(&id).cloned())
                        .ok_or(Unsupported)?,
                    None => self.slots.get(id as usize).cloned().ok_or(Unsupported)?,
                }
            }
            Opcode::LoadItem | Opcode::LoadBound => {
                let depth = first.args[0] as usize;
                let frame = frames
                    .len()
                    .checked_sub(depth + 1)
                    .and_then(|i| frames.get(i))
                    .ok_or(Unsupported)?;
                (if op == Opcode::LoadItem {
                    &frame.item
                } else {
                    &frame.bound
                })
                .clone()
                .ok_or(Unsupported)?
            }
            _ => return Err(Unsupported),
        };
        loop {
            let step = binding_step(&mut r)?;
            match step.op {
                Opcode::Return if r.peek().is_none() => return Ok(value),
                Opcode::Field => {
                    *fields += 1;
                    if *fields > 8 {
                        return Err(Limit);
                    }
                    let index = step.args[0] as usize;
                    let Value::Record(record) = &value else {
                        return Err(Unsupported);
                    };
                    value = record.get(index).cloned().ok_or(Unsupported)?;
                }
                _ => return Err(Unsupported),
            }
        }
    }
}
