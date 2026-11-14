//! Resolve named paths in the positional values both evaluators print (LLP 1151 D1).

use exact_plan::{Plan, TypeKind};

const MAX_DEPTH: usize = exact_runner::vm::MAX_VALUE_DEPTH as usize;

/// Missing roots, fields and indices, and paths through scalars or `none`, have no value.
pub(crate) fn value<'a>(plan: &Plan, step: &'a [String], path: &str) -> Option<&'a str> {
    let mut parts = path.split('.');
    let name = parts.next()?;
    let (kind, mut ty) = plan
        .slots
        .iter()
        .find(|s| s.owner.is_none() && plan.str(s.name) == name)
        .map(|s| ("slot", s.ty))
        .or_else(|| {
            plan.derives
                .iter()
                .find(|d| plan.str(d.name) == name)
                .map(|d| ("derive", d.ty))
        })
        .or_else(|| {
            plan.resources
                .iter()
                .find(|r| plan.str(r.name) == name)
                .map(|r| ("resource", r.ty))
        })?;
    let prefix = format!("{kind} {name} ");
    let mut text = step.iter().find_map(|l| l.strip_prefix(&prefix))?;
    let mut depth = 0;
    loop {
        while plan.type_(ty).kind == TypeKind::Option {
            depth += 1;
            if depth > MAX_DEPTH {
                return None;
            }
            if text == "none" {
                return parts.next().is_none().then_some(text);
            }
            text = text.strip_prefix("some(")?.strip_suffix(')')?;
            ty = plan.type_(ty).elem?;
        }
        let Some(part) = parts.next() else {
            return Some(text);
        };
        if part.is_empty() {
            return None;
        }
        depth += 1;
        if depth > MAX_DEPTH {
            return None;
        }
        let row = plan.type_(ty);
        let (index, child, open, close) = match row.kind {
            TypeKind::Record => {
                let (index, field) = row
                    .fields
                    .iter()
                    .map(|id| plan.field(id))
                    .enumerate()
                    .find(|(_, f)| plan.str(f.name) == part)?;
                (index, field.ty, '{', '}')
            }
            TypeKind::List => {
                let index = part.parse::<usize>().ok()?;
                if index.to_string() != part {
                    return None;
                }
                (index, row.elem?, '[', ']')
            }
            _ => return None,
        };
        text = item(text, index, open, close)?;
        ty = child;
    }
}

fn item(text: &str, index: usize, open: char, close: char) -> Option<&str> {
    let mut rest = text.strip_prefix(open)?.strip_suffix(close)?;
    for i in 0..=index {
        let len = value_len(rest, 0)?;
        if i == index {
            return (rest.len() == len || rest.as_bytes().get(len) == Some(&b','))
                .then_some(&rest[..len]);
        }
        rest = rest[len..].strip_prefix(',')?;
    }
    None
}

/// Skip one canonical value without allocating a tree or splitting inside strings.
fn value_len(text: &str, depth: usize) -> Option<usize> {
    if depth > MAX_DEPTH {
        return None;
    }
    let bytes = text.as_bytes();
    match *bytes.first()? {
        b'"' => {
            let mut i = 1;
            while i < bytes.len() {
                match bytes[i] {
                    b'"' => return Some(i + 1),
                    b'\\' => i += 1,
                    _ => {}
                }
                i += 1;
            }
            None
        }
        b'[' | b'{' => {
            let close = if bytes[0] == b'[' { b']' } else { b'}' };
            let mut i = 1;
            if bytes.get(i) == Some(&close) {
                return Some(i + 1);
            }
            loop {
                i += value_len(text.get(i..)?, depth + 1)?;
                match bytes.get(i)? {
                    b',' => i += 1,
                    b if *b == close => return Some(i + 1),
                    _ => return None,
                }
            }
        }
        b's' if text.starts_with("some(") => {
            let len = 5 + value_len(&text[5..], depth + 1)?;
            (bytes.get(len) == Some(&b')')).then_some(len + 1)
        }
        b'n' if bytes
            .get(1..17)
            .is_some_and(|hex| hex.iter().all(u8::is_ascii_hexdigit)) =>
        {
            Some(17)
        }
        _ => ["none", "true", "false", "()"]
            .iter()
            .find(|v| text.starts_with(**v))
            .map(|v| v.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skipping_values_is_bounded_and_preserves_nested_delimiters() {
        assert_eq!(item("[none,true]", 1, '[', ']'), Some("true"));
        assert_eq!(item("[(),false]", 1, '[', ']'), Some("false"));
        assert_eq!(
            item("[some({\"[,\\\"}\",[]}),true]", 1, '[', ']'),
            Some("true")
        );
        for text in ["[", "[\"unterminated,true]", "[{],true]", "[some(),true]"] {
            assert_eq!(item(text, 1, '[', ']'), None, "{text}");
        }
        for (depth, expected) in [(MAX_DEPTH, Some("true")), (MAX_DEPTH + 1, None)] {
            let nested = format!("{}true{}", "[".repeat(depth), "]".repeat(depth));
            assert_eq!(item(&format!("[{nested},true]"), 1, '[', ']'), expected);
        }
    }
}
