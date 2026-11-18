use crate::{Error, Layout, Location, Member, UniformType, ARGUMENTS};

#[derive(Clone, Copy)]
struct Token<'a> {
    text: &'a str,
    offset: usize,
}

struct Reader<'a> {
    source: &'a str,
    tokens: Vec<Token<'a>>,
}

/// Find the uniform's struct, including forward declarations, and apply WGSL's layout.
/// Other structs and function bodies do not declare this surface's uniforms.
pub fn parse(source: &str) -> Result<Layout, Error> {
    let reader = Reader::new(source)?;
    let mut structs = Vec::new();
    let mut aliases = Vec::new();
    let mut uniform = None;
    let mut i = 0;
    while i < reader.tokens.len() {
        match reader.text(i) {
            "alias" => {
                aliases.push(reader.identifier(i + 1)?);
                i += 2;
            }
            "struct" => {
                let name = reader.identifier(i + 1)?;
                reader.expect(i + 2, "{")?;
                let end = reader.close(i + 2, "{", "}")?;
                if structs.iter().any(|(n, _, _)| *n == name) {
                    return Err(reader.error(i + 1, format!("duplicate struct `{name}`")));
                }
                structs.push((name, i + 3, end));
                i = end + 1;
            }
            "var" if reader.text(i + 1) == "<" && reader.text(i + 2) == "uniform" => {
                i += 3;
                i += usize::from(reader.text(i) == ",");
                reader.expect(i, ">")?;
                let name = reader.identifier(i + 1)?;
                reader.expect(i + 2, ":")?;
                let ty = reader.identifier(i + 3)?;
                reader.expect(i + 4, ";")?;
                if uniform.is_some() {
                    return Err(reader.error(i + 1, format!("second uniform binding `{name}`")));
                }
                uniform = Some((ty, i + 3));
                i += 5;
            }
            "{" => i = reader.close(i, "{", "}")? + 1,
            _ => i += 1,
        }
    }
    let mut layout = Layout {
        name: None,
        members: Vec::new(),
        alignment: 1,
        size: 0,
    };
    let Some((name, at)) = uniform else {
        return Ok(layout);
    };
    let Some((_, start, end)) = structs.iter().find(|(n, _, _)| *n == name) else {
        return Err(reader.error(
            at,
            format!("uniform type `{name}` must be a declared struct"),
        ));
    };
    layout.name = Some(name.to_string());
    let mut i = *start;
    while i < *end {
        let mut attributes = Vec::new();
        while reader.text(i) == "@" {
            attributes.push(reader.identifier(i + 1)?);
            i += 2;
            if reader.text(i) == "(" {
                i = reader.close(i, "(", ")")? + 1;
            }
        }
        let name = reader.identifier(i)?;
        let at = i;
        reader.expect(i + 1, ":")?;
        if let Some(attr) = attributes
            .into_iter()
            .find(|attr| matches!(*attr, "align" | "size"))
        {
            return Err(reader.error(at, format!("uniform member `{name}` cannot have @{attr}")));
        }
        i += 2;
        let start = i;
        let mut depth = 0;
        while i < *end && !(depth == 0 && reader.text(i) == ",") {
            match reader.text(i) {
                "<" => depth += 1,
                ">" => depth -= 1,
                _ => {}
            }
            i += 1;
        }
        let ty: Vec<_> = reader.tokens[start..i].iter().map(|t| t.text).collect();
        if ty
            .iter()
            .any(|name| aliases.contains(name) || structs.iter().any(|(n, _, _)| n == name))
        {
            return Err(reader.error(
                at,
                format!("uniform member `{name}` cannot use an alias or nested struct"),
            ));
        }
        let ty = match ty.as_slice() {
            ["f32"] => UniformType::F32,
            ["i32"] => UniformType::I32,
            ["u32"] => UniformType::U32,
            ["vec2f"] | ["vec2", "<", "f32", ">"] | ["vec2", "<", "f32", ",", ">"] => {
                UniformType::Vec2f
            }
            ["vec3f"] | ["vec3", "<", "f32", ">"] | ["vec3", "<", "f32", ",", ">"] => {
                UniformType::Vec3f
            }
            ["vec4f"] | ["vec4", "<", "f32", ">"] | ["vec4", "<", "f32", ",", ">"] => {
                UniformType::Vec4f
            }
            _ => {
                return Err(reader.error(
                    at,
                    format!(
                        "uniform member `{name}` has unsupported type `{}`",
                        ty.join("")
                    ),
                ))
            }
        };
        if ARGUMENTS.contains(&name) {
            return Err(reader.error(at, format!("uniform member `{name}` is a surface argument")));
        }
        let builtin = match name {
            "time" | "devicePixelRatio" => Some(UniformType::F32),
            "resolution" => Some(UniformType::Vec2f),
            _ => None,
        };
        if let Some(required) = builtin {
            if ty != required {
                return Err(reader.error(
                    at,
                    format!(
                        "built-in uniform `{name}` must have type {}",
                        required.name()
                    ),
                ));
            }
        }
        if layout.members.iter().any(|m| m.name == name) {
            return Err(reader.error(at, format!("duplicate uniform member `{name}`")));
        }
        let offset = layout.size.next_multiple_of(ty.alignment());
        layout.size = offset + ty.size();
        layout.alignment = layout.alignment.max(ty.alignment());
        layout.members.push(Member {
            name: name.into(),
            ty,
            offset,
            location: reader.location(reader.tokens[at].offset),
        });
        i += usize::from(reader.text(i) == ",");
    }
    if layout.members.is_empty() {
        return Err(reader.error(at, format!("uniform struct `{name}` has no members")));
    }
    layout.size = layout.size.next_multiple_of(layout.alignment);
    Ok(layout)
}

impl<'a> Reader<'a> {
    fn new(source: &'a str) -> Result<Self, Error> {
        let mut reader = Self {
            source,
            tokens: Vec::new(),
        };
        let mut i = 0;
        while i < source.len() {
            let rest = &source[i..];
            if rest.starts_with("//") {
                i += rest.find(line_break).unwrap_or(rest.len());
            } else if rest.starts_with("/*") {
                let start = i;
                let mut depth = 1;
                i += 2;
                while i < source.len() && depth != 0 {
                    if source[i..].starts_with("/*") {
                        depth += 1;
                        i += 2;
                    } else if source[i..].starts_with("*/") {
                        depth -= 1;
                        i += 2;
                    } else {
                        i += source[i..].chars().next().unwrap().len_utf8();
                    }
                }
                if depth != 0 {
                    return Err(Error {
                        message: "unterminated WGSL comment".into(),
                        location: Some(reader.location(start)),
                    });
                }
            } else {
                let c = rest.chars().next().unwrap();
                if blankspace(c) {
                    i += c.len_utf8();
                    continue;
                }
                let start = i;
                i += c.len_utf8();
                if word(c) {
                    while i < source.len() {
                        let c = source[i..].chars().next().unwrap();
                        if !word(c) {
                            break;
                        }
                        i += c.len_utf8();
                    }
                }
                reader.tokens.push(Token {
                    text: &source[start..i],
                    offset: start,
                });
            }
        }
        Ok(reader)
    }

    fn text(&self, i: usize) -> &'a str {
        self.tokens.get(i).map_or("", |t| t.text)
    }

    fn identifier(&self, i: usize) -> Result<&'a str, Error> {
        let text = self.text(i);
        if text
            .chars()
            .next()
            .is_some_and(|c| word(c) && !c.is_ascii_digit())
        {
            Ok(text)
        } else {
            Err(self.error(i, "expected a WGSL identifier".into()))
        }
    }

    fn expect(&self, i: usize, text: &str) -> Result<(), Error> {
        if self.text(i) == text {
            Ok(())
        } else {
            Err(self.error(i, format!("expected `{text}`, found `{}`", self.text(i))))
        }
    }

    fn close(&self, start: usize, open: &str, close: &str) -> Result<usize, Error> {
        let mut depth = 0;
        for i in start..self.tokens.len() {
            if self.text(i) == open {
                depth += 1;
            } else if self.text(i) == close {
                depth -= 1;
                if depth == 0 {
                    return Ok(i);
                }
            }
        }
        Err(self.error(start, format!("unclosed `{open}`")))
    }

    fn location(&self, offset: usize) -> Location {
        let mut chars = self.source[..offset].chars().peekable();
        let (mut line, mut column) = (1, 1);
        while let Some(c) = chars.next() {
            if line_break(c) {
                line += 1;
                column = 1;
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
            } else {
                column += 1;
            }
        }
        Location {
            offset,
            line,
            column,
        }
    }

    fn error(&self, i: usize, message: String) -> Error {
        let offset = self.tokens.get(i).map_or(self.source.len(), |t| t.offset);
        Error {
            message,
            location: Some(self.location(offset)),
        }
    }
}

// WGSL §3.2: https://www.w3.org/TR/WGSL/#blankspace-and-line-breaks
fn blankspace(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\u{200e}' | '\u{200f}') || line_break(c)
}

fn line_break(c: char) -> bool {
    matches!(
        c,
        '\n' | '\u{b}' | '\u{c}' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}'
    )
}

fn word(c: char) -> bool {
    c == '_' || (!c.is_ascii_punctuation() && !blankspace(c))
}
