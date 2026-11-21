use crate::instance::place::Part;
use crate::json::{quote, untyped_json};
use std::fmt::Write;

pub(super) fn write(path: &[Part], out: &mut String) {
    out.push('[');
    for (index, part) in path.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push('{');
        match part {
            Part::Component(name, index) | Part::Element(name, index) => {
                out.push_str(if matches!(part, Part::Component(..)) {
                    "\"component\":"
                } else {
                    "\"element\":"
                });
                quote(name, out);
                let _ = write!(out, ",\"index\":{index}");
            }
            Part::Id(id) => {
                out.push_str("\"id\":");
                quote(id, out);
            }
            Part::Each(index, key) => {
                let _ = write!(out, "\"for\":{index},\"keyExpression\":");
                quote(key, out);
            }
            Part::Children(index) => {
                let _ = write!(out, "\"children\":{index}");
            }
            Part::Row(key, repeat) => {
                out.push_str("\"key\":");
                untyped_json(key, out);
                let _ = write!(out, ",\"repeat\":{repeat}");
            }
        }
        out.push('}');
    }
    out.push(']');
}
