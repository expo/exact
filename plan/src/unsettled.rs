use crate::{DerivesRow, Opcode, Operand, Plan, PlanError};

impl Plan {
    /// The body and owner behind an at-read derive load; settled loads stay values.
    /// Static readers use this accessor before interpreting a load as an opaque value.
    pub fn unsettled_body(&self, index: u32) -> Option<&DerivesRow> {
        self.derives.get(index as usize).filter(|row| row.unsettled)
    }

    /// Follow a body consisting of one at-read load. Shape readers receive
    /// the resolved body and its owner; inner loads remain explicit.
    pub fn read_code<'a>(
        &'a self,
        mut code: &'a [u8],
    ) -> (&'a [u8], Option<Option<crate::ArmsId>>) {
        let mut owner = None;
        for _ in 0..64 {
            let Some(row) = self.executable_alias(code) else {
                break;
            };
            code = self.code(row.body);
            owner = Some(row.owner);
        }
        (code, owner)
    }

    fn executable_alias(&self, code: &[u8]) -> Option<&DerivesRow> {
        let [op, a, b, c, d, ret] = code else {
            return None;
        };
        (*op == Opcode::LoadDerive as u8 && *ret == Opcode::Return as u8)
            .then(|| self.unsettled_body(u32::from_le_bytes([*a, *b, *c, *d])))?
    }

    /// Resolve an executable alias in the frames its owner can read.
    pub fn read_code_in<'a, 'f, T>(
        &'a self,
        code: &'a [u8],
        frames: &'f [T],
        region: impl Fn(&T) -> Option<u32>,
    ) -> Option<(&'a [u8], &'f [T])> {
        let mut code = code;
        let mut frames = frames;
        let mut calls = 0;
        while let Some(row) = self.executable_alias(code) {
            calls += 1;
            if calls > 64 {
                return None;
            }
            frames = self.owner_frames(frames, row.owner, &region)?;
            code = self.code(row.body);
        }
        Some((code, frames))
    }

    /// The lexical frame prefix available to an executable row, in the VM
    /// and in static projections. A root row reads no instance frames.
    pub fn owner_frames<'f, T>(
        &self,
        frames: &'f [T],
        owner: Option<crate::ArmsId>,
        region: impl Fn(&T) -> Option<u32>,
    ) -> Option<&'f [T]> {
        let end = match owner {
            None => 0,
            Some(owner) => {
                let target = self.arm(owner).region.0;
                frames.iter().rposition(|f| region(f) == Some(target))? + 1
            }
        };
        Some(&frames[..end])
    }

    /// Frames removed when a reader calls a row owned by an enclosing arm.
    /// `None` means that owner is outside the reader's lexical scope.
    pub fn frame_cut(
        &self,
        mut reader: Option<crate::ArmsId>,
        owner: Option<crate::ArmsId>,
    ) -> Option<u32> {
        let mut cut = 0;
        while reader != owner {
            reader = self.region(self.arm(reader?).region).arm;
            cut += 1;
        }
        Some(cut)
    }

    pub(crate) fn validate_unsettled(&self) -> Result<(), PlanError> {
        for (i, row) in self.derives.iter().enumerate() {
            if !row.unsettled {
                if row.owner.is_some() {
                    return Err(PlanError::BadReference {
                        table: "derives",
                        row: i as u32,
                        field: "owner",
                    });
                }
                continue;
            }
            let mut reader = crate::bytes::Reader::new(self.code(row.body));
            while !reader.is_empty() {
                let op = Opcode::from_wire(reader.u8()?).expect("validated opcode");
                for operand in op.operands() {
                    match operand {
                        Operand::U8 | Operand::Enum(_) => {
                            reader.u8()?;
                        }
                        Operand::U16 => {
                            reader.u16()?;
                        }
                        Operand::U32 | Operand::Str | Operand::Idx(_) => {
                            let n = reader.u32()?;
                            if op == Opcode::LoadDerive
                                && self.unsettled_body(n).is_some()
                                && n as usize >= i
                            {
                                return Err(PlanError::DeriveOrder {
                                    row: i as u32,
                                    dependency: n,
                                });
                            }
                        }
                        Operand::F64 => {
                            reader.f64()?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{asm::Asm, builder::PlanBuilder, DerivesId, PlanError, TypeKind, Value};

    #[test]
    fn static_alias_read_stops_at_the_runtime_call_depth() {
        use crate::Opcode;
        let mut b = PlanBuilder::new(0, 1);
        let ty = b.primitive(TypeKind::Number);
        let body = b.constant(&Value::Number(1.0));
        let mut row = b.derive("base", ty, body);
        b.set_derive_unsettled(row, None);
        for _ in 1..65 {
            let mut a = Asm::default();
            a.load_derive(row);
            let body = b.code(a);
            row = b.derive("alias", ty, body);
            b.set_derive_unsettled(row, None);
        }
        let mut a = Asm::default();
        a.load_derive(row);
        let reader = b.code(a);
        let p = b.finish().unwrap();
        let (code, _) = p.read_code(p.code(reader));
        assert_eq!(code[0], Opcode::LoadDerive as u8);
        assert!(p
            .read_code_in(p.code(reader), &[] as &[u32], |f| Some(*f))
            .is_none());
        let (code, _) = p.read_code(code);
        assert_eq!(code[0], Opcode::Number as u8);
    }

    #[test]
    fn executable_rows_refuse_self_and_forward_calls() {
        for dependency in [0, 1] {
            let mut b = PlanBuilder::new(0, 1);
            let ty = b.primitive(TypeKind::Number);
            let mut a = Asm::default();
            a.load_derive(DerivesId(dependency));
            let code = b.code(a);
            let first = b.derive("first", ty, code);
            b.set_derive_unsettled(first, None);
            let code = b.constant(&Value::Number(1.0));
            let second = b.derive("second", ty, code);
            b.set_derive_unsettled(second, None);
            assert!(matches!(
                b.finish(),
                Err(PlanError::DeriveOrder { row: 0, .. })
            ));
        }
    }
}
