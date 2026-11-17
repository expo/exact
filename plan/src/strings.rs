//! Localized texts (LLP 1060): the tables `t(...)` reads, the chain that
//! picks a table for the viewer, and the one placeholder grammar the
//! compiler checks and the runner fills.

use crate::{LocalesRow, Plan, PlanError, TypeKind};

/// Validate the admitted MF2 simple-message subset before a table is baked.
/// `{name}` is the Contract shorthand for MF2's `{$name}`.
pub fn validate_message(text: &str) -> Result<(), String> {
    for piece in pieces(text) {
        piece?;
    }
    Ok(())
}

/// Variable names in a validated message, in order, excluding escaped braces.
pub fn placeholders(text: &str) -> impl Iterator<Item = &str> {
    pieces(text).filter_map(|piece| match piece {
        Ok(Piece::Name { name, .. }) => Some(name),
        _ => None,
    })
}

/// Fill a validated message, refusing malformed syntax and an expansion longer
/// than `limit` bytes before allocating it. Unfilled variables keep their spelling.
pub fn fill<'v>(
    text: &str,
    value: impl Fn(&str) -> Option<&'v str>,
    limit: usize,
) -> Option<String> {
    let mut resolved = Vec::new();
    let mut len = 0usize;
    for piece in pieces(text) {
        let text = match piece.ok()? {
            Piece::Name { name, raw } => value(name).unwrap_or(raw),
            Piece::Text(text) => text,
        };
        len = len.checked_add(text.len()).filter(|len| *len <= limit)?;
        resolved.push(text);
    }
    let mut out = String::with_capacity(len);
    for text in resolved {
        out.push_str(text);
    }
    Some(out)
}

enum Piece<'a> {
    Text(&'a str),
    Name { name: &'a str, raw: &'a str },
}

fn pieces(text: &str) -> impl Iterator<Item = Result<Piece<'_>, String>> {
    let mut rest = text;
    let mut first = true;
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        let result = (|| {
            if first {
                first = false;
                if rest.trim_start().starts_with('.') {
                    let keyword = rest.split_whitespace().next().unwrap();
                    return Err(format!(
                        "MF2 `{keyword}` declarations/matching are not implemented"
                    ));
                }
            }
            let end = rest.find(['{', '}', '\\', '\0']).unwrap_or(rest.len());
            if end > 0 {
                let text = &rest[..end];
                rest = &rest[end..];
                return Ok(Piece::Text(text));
            }
            if rest.starts_with('\\') {
                if !matches!(rest.as_bytes().get(1), Some(b'{' | b'}' | b'\\')) {
                    return Err("invalid MF2 escape; escape only braces and backslashes".into());
                }
                let text = &rest[1..2];
                rest = &rest[2..];
                return Ok(Piece::Text(text));
            }
            if !rest.starts_with('{') {
                return Err("unescaped closing brace or NUL in MF2 text".into());
            }
            if rest.starts_with("{{") {
                return Err("MF2 quoted pattern is not implemented".into());
            }
            let end = rest.find('}').ok_or("unclosed MF2 brace")?;
            let expression = rest[1..end].trim();
            if expression.starts_with(['#', '/']) {
                return Err("MF2 markup is not implemented".into());
            }
            if let Some(at) = expression.find(':') {
                let function = expression[at..].split_whitespace().next().unwrap();
                return Err(format!("MF2 function `{function}` is not implemented"));
            }
            if expression.contains(',') {
                return Err("plural/select syntax is not implemented; tables use MF2".into());
            }
            if expression.contains('@') {
                return Err("MF2 attributes are not implemented".into());
            }
            let name = expression.strip_prefix('$').unwrap_or(expression);
            if !name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
                || !name
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
            {
                return Err(
                    "MF2 literal or expression is not implemented; use {$name} or {name}".into(),
                );
            }
            let raw = &rest[..=end];
            rest = &rest[end + 1..];
            Ok(Piece::Name { name, raw })
        })();
        if result.is_err() {
            rest = "";
        }
        Some(result)
    })
}

/// The hour cycle a locale's Unicode extension forces (`hc-h23` in
/// `ca-gregory-hc-h23`): `h12` or `h23`, the two the format data carries;
/// `h11` and `h24` force none.
pub fn hour_cycle(extension: &str) -> Option<&str> {
    let mut subtags = extension.split('-');
    while let Some(key) = subtags.next() {
        if key.eq_ignore_ascii_case("hc") {
            return match subtags.next()? {
                c if c.eq_ignore_ascii_case("h12") => Some("h12"),
                c if c.eq_ignore_ascii_case("h23") => Some("h23"),
                _ => None,
            };
        }
    }
    None
}

/// The hour cycle `tag`'s Unicode extension forces (`en-US-u-hc-h23`).
fn tag_hour_cycle(tag: &str) -> Option<&str> {
    hour_cycle(tag.find("-u-").map_or("", |at| &tag[at + 3..]))
}

/// The format locale (LLP 1054.000.003 D6): the resolved table's whole
/// name, then, apart by a space no tag holds, the hour cycle the viewer's
/// regional format forces (`exactTime()`'s `formatLocale`,
/// `en-US-u-hc-h23`), unless the table's own name forces one: `de h23`,
/// `de-u-ca-gregory`.
pub fn format_tag(table: &str, format_locale: &str) -> String {
    match tag_hour_cycle(format_locale).filter(|_| tag_hour_cycle(table).is_none()) {
        Some(cycle) => format!("{table} {cycle}"),
        None => table.to_owned(),
    }
}

impl Plan {
    /// The table the viewer's `requested` locale (BCP 47) reads: the
    /// longest subtag prefix a table is named for, compared without case
    /// (RFC 4647 lookup: `zh-Hant-TW`, `zh-Hant`, `zh`), else the base.
    /// `None` when the plan has no strings.
    pub fn resolve_locale(&self, requested: &str) -> Option<&str> {
        let base = self.locales.first()?;
        let mut tag = requested;
        loop {
            if let Some(row) = self
                .locales
                .iter()
                .find(|row| self.str(row.name).eq_ignore_ascii_case(tag))
            {
                return Some(self.str(row.name));
            }
            match tag.rfind('-') {
                Some(end) => tag = &tag[..end],
                None => return Some(self.str(base.name)),
            }
        }
    }

    /// `key`'s text in the table named `locale`, or in the base when that
    /// table lacks it (a translation that is behind is still readable).
    pub fn localized(&self, locale: &str, key: &str) -> Option<&str> {
        let find = |row: &LocalesRow| {
            let texts = &self.texts[row.texts.start as usize..][..row.texts.len as usize];
            texts
                .binary_search_by(|t| self.str(t.key).cmp(key))
                .ok()
                .map(|i| self.str(texts[i].text))
        };
        self.locales
            .iter()
            .find(|row| self.str(row.name) == locale)
            .and_then(find)
            .or_else(|| find(self.locales.first()?))
    }

    /// The format data of the table a format locale names ([`format_tag`]'s
    /// `de h23`), and the hour cycle it forces, `h12` or `h23`: the one
    /// after the space, else the table name's own (LLP 1054.000.003 D6).
    /// `None` for a table the plan does not have or that carries no data.
    pub fn format_data<'a>(&'a self, locale: &'a str) -> Option<(&'a str, Option<&'a str>)> {
        let (table, cycle) = match locale.split_once(' ') {
            Some((table, cycle)) => (table, Some(cycle).filter(|c| matches!(*c, "h12" | "h23"))),
            None => (locale, tag_hour_cycle(locale)),
        };
        let data = self
            .locales
            .iter()
            .find(|row| self.str(row.name) == table)
            .map(|row| self.str(row.format))
            .filter(|data| !data.is_empty())?;
        Some((data, cycle))
    }

    /// Strings and their slot come together; each table is sorted by key
    /// (lookup is a binary search) and named once.
    pub(crate) fn validate_texts(&self) -> Result<(), PlanError> {
        let bad = |table, row: usize, field| PlanError::BadReference {
            table,
            row: row as u32,
            field,
        };
        match self.locale {
            Some(id) => {
                let slot = self.slot(id);
                if self.type_(slot.ty).kind != TypeKind::String
                    || slot.owner.is_some()
                    || self.locales.is_empty()
                {
                    return Err(bad("header", 0, "locale"));
                }
            }
            None if !self.locales.is_empty() => return Err(bad("locales", 0, "name")),
            None => {}
        }
        // The format slot comes with format data in every row, as the
        // compiler bakes it (LLP 1054.000.003 D6).
        let formats = self
            .locales
            .iter()
            .filter(|row| !self.str(row.format).is_empty())
            .count();
        match self.format {
            Some(id) => {
                let slot = self.slot(id);
                if self.type_(slot.ty).kind != TypeKind::String
                    || slot.owner.is_some()
                    || slot.late
                    || slot.persist
                    || self.locale == Some(id)
                    || formats != self.locales.len()
                    || formats == 0
                {
                    return Err(bad("header", 0, "format"));
                }
            }
            None if formats > 0 => return Err(bad("locales", 0, "format")),
            None => {}
        }
        for (i, row) in self.locales.iter().enumerate() {
            let name = self.str(row.name);
            if name.is_empty()
                || self.locales[..i]
                    .iter()
                    .any(|other| self.str(other.name).eq_ignore_ascii_case(name))
            {
                return Err(bad("locales", i, "name"));
            }
            let texts = &self.texts[row.texts.start as usize..][..row.texts.len as usize];
            if texts
                .iter()
                .any(|text| validate_message(self.str(text.text)).is_err())
            {
                return Err(bad("locales", i, "texts"));
            }
            if texts
                .windows(2)
                .any(|pair| self.str(pair[0].key) >= self.str(pair[1].key))
            {
                return Err(bad("locales", i, "texts"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mf2_names_and_escapes_are_shared_by_validation_and_filling() {
        let text = r"Hi {$name}, {count} \{literal\} \\";
        validate_message(text).unwrap();
        assert_eq!(placeholders(text).collect::<Vec<_>>(), ["name", "count"]);
        assert_eq!(
            fill(text, |_| Some("Ada"), 100).unwrap(),
            r"Hi Ada, Ada {literal} \"
        );
        assert!(fill("{$n :number}", |_| Some("1"), 100).is_none());
        assert_eq!(
            fill("{$missing}", |_| None, 10).as_deref(),
            Some("{$missing}")
        );
    }

    #[test]
    fn expansion_counts_bytes_literals_and_unfilled_names_before_allocating() {
        assert_eq!(fill("{x}{x}", |_| Some("é"), 4).as_deref(), Some("éé"));
        assert_eq!(fill("{x}{x}", |_| Some("é"), 3), None);
        assert_eq!(fill("!{x}", |_| Some("abc"), 3), None);
        assert_eq!(fill("{missing}", |_| None, 8), None);
        assert_eq!(fill("{missing}", |_| None, 9).as_deref(), Some("{missing}"));
        assert_eq!(fill("literal", |_| None, 6), None);
        assert_eq!(fill("{x}", |_| Some(""), 0).as_deref(), Some(""));
    }
}
