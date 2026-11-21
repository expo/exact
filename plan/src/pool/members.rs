//! A component's members share their owner's SHA-256 namespace. A qualified
//! name reads exactly as it was interned, including in an author's string literal.
use crate::bytes::{Reader, Writer, MAX_COUNT};
use crate::PlanError;
use std::collections::HashMap;
use std::fmt::Write;
use std::sync::OnceLock;

pub(super) const QUALIFIED: u32 = 1 << 31;

#[derive(Default)]
pub(super) struct Encoder<'a> {
    names: HashMap<&'a str, u32>,
    owners: HashMap<&'a str, u32>,
    expanded: usize,
}

impl<'a> Encoder<'a> {
    pub fn write(&mut self, text: &'a str, w: &mut Writer) -> bool {
        let Some((name, owner)) = text.rsplit_once('#') else {
            return false;
        };
        if owner.len() != 64
            || !owner
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            || text.len() > MAX_COUNT - self.expanded
        {
            return false;
        }
        let names = self.names.len() as u32;
        let owners = self.owners.len() as u32;
        let member = self.names.get(name).copied().unwrap_or(names);
        let namespace = self.owners.get(owner).copied().unwrap_or(owners);
        self.expanded += text.len();
        w.u32(QUALIFIED | namespace);
        w.u32(member);
        if member == names {
            self.names.insert(name, member);
            w.string(name);
        }
        if namespace == owners {
            self.owners.insert(owner, namespace);
            for i in (0..64).step_by(2) {
                w.u8(u8::from_str_radix(&owner[i..i + 2], 16).expect("validated hex digest"));
            }
        }
        true
    }
}

#[derive(Default, Clone)]
pub(super) struct Members {
    names: Vec<String>,
    owners: Vec<[u8; 32]>,
    texts: HashMap<(usize, usize), OnceLock<String>>,
    expanded: usize,
}

impl Members {
    pub fn read(&mut self, owner: usize, index: u32, r: &mut Reader<'_>) -> Result<(), PlanError> {
        let name = r.u32()? as usize;
        if name > self.names.len() || owner > self.owners.len() {
            return Err(PlanError::BadReference {
                table: "strings",
                row: index,
                field: "member",
            });
        }
        if name == self.names.len() {
            self.names.push(r.string()?);
        }
        if owner == self.owners.len() {
            self.owners
                .push(r.bytes(32)?.try_into().expect("32 digest bytes"));
        }
        let size = self.names[name].len() + 65;
        if size > MAX_COUNT - self.expanded {
            return Err(PlanError::BadCount((self.expanded + size) as u32));
        }
        self.expanded += size;
        self.texts.entry((name, owner)).or_default();
        Ok(())
    }

    pub fn get(&self, name: usize, owner: usize) -> &str {
        self.texts[&(name, owner)].get_or_init(|| {
            let mut text = String::with_capacity(self.names[name].len() + 65);
            text.push_str(&self.names[name]);
            text.push('#');
            for byte in self.owners[owner] {
                write!(text, "{byte:02x}").unwrap();
            }
            text
        })
    }
}
