//! The plan's interned strings. A decoded plan reads them where its bytes
//! hold their literal bytes. A static plan lends its pool and a loaded plan
//! keeps one copy. A qualified member name is joined on its first read. A
//! compiler or a later insertion supplies an owned `String`. A `Vec<String>` cost a
//! euxy boot 1,331 allocations and some 70 KB for its life (2026-10-19).

mod members;

use crate::{
    bytes::{Reader, Writer, MAX_COUNT},
    PlanError,
};
use members::{Encoder, Members, QUALIFIED};
use std::borrow::Cow;

/// A plan's strings by id: the pool its bytes decoded to, then those interned since.
#[derive(Clone, Default)]
pub struct Strings {
    /// The encoded string records, each validated
    /// when decoded and never written after.
    pool: Cow<'static, [u8]>,
    /// Each record's offset into `pool`.
    at: Vec<u32>,
    members: Option<Box<Members>>,
    /// Strings interned after the decode; their ids follow the pool's.
    extra: Vec<String>,
}

impl Strings {
    pub(crate) fn encode(&self, w: &mut Writer) {
        w.u32(self.len() as u32);
        let mut members = Encoder::default();
        for text in self.iter() {
            if !members.write(text, w) {
                w.string(text);
            }
        }
    }

    /// Decode the pool at `r`, which reads `whole`; `lent` is `whole` when it
    /// lives as long as the program, so the pool stays in it.
    pub(crate) fn decode(
        r: &mut Reader<'_>,
        whole: &[u8],
        lent: Option<&'static [u8]>,
    ) -> Result<Self, PlanError> {
        let n = r.count()?;
        let start = whole.len() - r.remaining();
        let mut at = Vec::with_capacity(r.reserve(n, 4));
        let mut members = None;
        for index in 0..n {
            let offset = r.position() - start;
            at.push(u32::try_from(offset).map_err(|_| PlanError::BadCount(u32::MAX))?);
            let tag = r.u32()?;
            if tag & QUALIFIED != 0 {
                members.get_or_insert_with(Box::<Members>::default).read(
                    (tag - QUALIFIED) as usize,
                    index as u32,
                    r,
                )?;
            } else {
                let len = tag as usize;
                if len > MAX_COUNT || len > r.remaining() {
                    return Err(PlanError::BadCount(tag));
                }
                std::str::from_utf8(r.bytes(len)?).map_err(|_| PlanError::BadUtf8)?;
            }
        }
        let end = whole.len() - r.remaining();
        let pool = match lent {
            Some(lent) => Cow::Borrowed(&lent[start..end]),
            None => Cow::Owned(whole[start..end].to_vec()),
        };
        Ok(Strings {
            pool,
            at,
            members,
            extra: Vec::new(),
        })
    }

    /// How many strings have ids.
    pub fn len(&self) -> usize {
        self.at.len() + self.extra.len()
    }

    /// Whether none has.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The string with id `id`, if there is one.
    pub fn get(&self, id: usize) -> Option<&str> {
        match self.at.get(id) {
            Some(&at) => {
                let at = at as usize;
                let tag = u32::from_le_bytes(self.pool.get(at..at + 4)?.try_into().ok()?);
                if tag & QUALIFIED != 0 {
                    let name = u32::from_le_bytes(self.pool.get(at + 4..at + 8)?.try_into().ok()?);
                    return Some(
                        self.members
                            .as_ref()?
                            .get(name as usize, (tag - QUALIFIED) as usize),
                    );
                }
                let text = at + 4;
                std::str::from_utf8(self.pool.get(text..text + tag as usize)?).ok()
            }
            None => self.extra.get(id - self.at.len()).map(String::as_str),
        }
    }

    /// Every string, by id.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &str> + DoubleEndedIterator + '_ {
        (0..self.len()).map(|id| &self[id])
    }

    /// Intern `s` with the next id, `len()` before the call.
    pub fn push(&mut self, s: impl Into<String>) {
        self.extra.push(s.into());
    }

    /// Intern each, with the ids from `len()` on.
    pub fn extend<I: IntoIterator<Item = String>>(&mut self, strings: I) {
        self.extra.extend(strings);
    }

    /// Replace the string with id `id`.
    pub fn set(&mut self, id: usize, s: impl Into<String>) {
        if id < self.at.len() {
            // A pooled string moves out of the pool: the ids after it stay.
            let pooled: Vec<String> = self.iter().map(str::to_owned).collect();
            self.pool = Cow::Borrowed(&[]);
            self.at = Vec::new();
            self.members = None;
            self.extra = pooled;
        }
        self.extra[id - self.at.len()] = s.into();
    }

    /// Which plan's strings these are, for a cache keyed by the plan: the
    /// address of the buffer that holds them.
    pub fn identity(&self) -> usize {
        if self.at.is_empty() {
            self.extra.as_ptr() as usize
        } else {
            self.pool.as_ptr() as usize
        }
    }
}

impl std::ops::Index<usize> for Strings {
    type Output = str;
    fn index(&self, id: usize) -> &str {
        self.get(id)
            .unwrap_or_else(|| panic!("string id {id} of {}", self.len()))
    }
}

impl<'a> IntoIterator for &'a Strings {
    type Item = &'a str;
    type IntoIter = Box<dyn Iterator<Item = &'a str> + 'a>;
    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.iter())
    }
}

impl std::fmt::Debug for Strings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl PartialEq for Strings {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytes::Writer;

    fn encoded(strings: &[&str]) -> Vec<u8> {
        let mut w = Writer::default();
        w.u32(strings.len() as u32);
        for s in strings {
            w.string(s);
        }
        w.u32(7);
        w.into_vec()
    }

    #[test]
    fn decoded_strings_read_in_place_and_extend() {
        let bytes = encoded(&["", "a", "héllo", "b"]);
        let mut r = Reader::new(&bytes);
        let mut strings = Strings::decode(&mut r, &bytes, None).unwrap();
        assert_eq!(r.remaining(), 4, "the pool ends where its last string does");
        assert_eq!(strings.len(), 4);
        assert_eq!(strings.get(2), Some("héllo"));
        assert_eq!(&strings[0], "");
        assert_eq!(strings.get(4), None);
        strings.push("after");
        assert_eq!(strings.get(4), Some("after"));
        assert_eq!(
            strings.iter().collect::<Vec<_>>(),
            ["", "a", "héllo", "b", "after"]
        );
        assert_eq!(strings.iter().position(|s| s == "b"), Some(3));
        assert_eq!(
            format!("{strings:?}"),
            r#"["", "a", "héllo", "b", "after"]"#
        );
        strings.set(1, "A");
        assert_eq!(
            strings.iter().collect::<Vec<_>>(),
            ["", "A", "héllo", "b", "after"]
        );
    }

    #[test]
    fn a_static_plan_lends_its_bytes() {
        let bytes: &'static [u8] = Box::leak(encoded(&["x", "yz"]).into_boxed_slice());
        let mut r = Reader::new(bytes);
        let strings = Strings::decode(&mut r, bytes, Some(bytes)).unwrap();
        let text = strings.get(1).unwrap();
        let range = bytes.as_ptr_range();
        assert!(
            range.contains(&text.as_ptr()),
            "the text is read from the static bytes"
        );
        assert!(matches!(strings.pool, Cow::Borrowed(_)));
    }

    #[test]
    fn qualified_names_round_trip_with_literals_and_extensions() {
        let first = "0123456789abcdef".repeat(4);
        let second = "f".repeat(64);
        let names = [
            "plain".into(),
            format!("draft#{first}"),
            format!("draft#{second}"),
            format!("é#part#{first}"),
            format!("literal#{}", first.to_uppercase()),
        ];
        let mut built = Strings::default();
        built.extend(names.iter().cloned());
        let mut wire = Writer::default();
        built.encode(&mut wire);
        let bytes = wire.into_vec();
        assert!(bytes.len() < names.iter().map(|s| s.len() + 4).sum::<usize>());
        let mut decoded = Strings::decode(&mut Reader::new(&bytes), &bytes, None).unwrap();
        assert_eq!(decoded, built);
        decoded.push("after");
        decoded.set(1, "renamed");
        assert_eq!(&decoded[1], "renamed");
        assert_eq!(&decoded[2], names[2]);
        assert_eq!(&decoded[5], "after");
    }

    #[test]
    fn a_qualified_name_must_define_its_parts_in_order() {
        let mut w = Writer::default();
        w.u32(1);
        w.u32(QUALIFIED | 1);
        w.u32(0);
        let bytes = w.into_vec();
        assert!(matches!(
            Strings::decode(&mut Reader::new(&bytes), &bytes, None),
            Err(PlanError::BadReference {
                table: "strings",
                row: 0,
                ..
            })
        ));
    }

    #[test]
    fn bad_utf8_is_refused_at_decode() {
        let mut w = Writer::default();
        w.u32(1);
        w.u32(2);
        w.bytes(&[0xff, 0xfe]);
        let bytes = w.into_vec();
        let mut r = Reader::new(&bytes);
        assert!(matches!(
            Strings::decode(&mut r, &bytes, None),
            Err(PlanError::BadUtf8)
        ));
    }

    #[test]
    fn equal_by_content_whatever_holds_them() {
        let bytes = encoded(&["p", "q"]);
        let decoded = Strings::decode(&mut Reader::new(&bytes), &bytes, None).unwrap();
        let mut built = Strings::default();
        built.extend(["p".to_string(), "q".to_string()]);
        assert_eq!(decoded, built);
        built.push("r");
        assert_ne!(decoded, built);
    }
}
