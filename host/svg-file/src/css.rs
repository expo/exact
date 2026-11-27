//! The styling a file carries (LLP 1011.001 D2): `style=""` on an element,
//! and a `style` element's rules with type, class and id selectors and no
//! combinators, which is what a designer's export writes (`.cls-1{fill:…}`).
//! A rule with anything else in its selector is dropped, with a warning.
//! Each declaration is applied as the presentation attribute of the same
//! name: the resolver sees one value per property, in the cascade's order
//! (attribute, then rules by specificity, then the style attribute).

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fmt;

/// Most stylesheet bytes read across all `<style>` elements in one SVG file.
pub const MAX_STYLE_BYTES: usize = 1024 * 1024;
/// Most rules read across all `<style>` elements in one SVG file.
pub const MAX_STYLE_RULES: usize = 10_000;
/// Most declarations parsed across all `<style>` elements in one SVG file.
pub const MAX_STYLE_DECLARATIONS: usize = 50_000;
/// Most selector checks and declarations applied across one SVG file.
pub const MAX_STYLE_WORK: usize = 1_000_000;
/// Most warnings retained for one SVG file.
pub const MAX_WARNINGS: usize = 256;
/// Most classes allowed on a single selector.
const MAX_SELECTOR_CLASSES: usize = 64;

/// A stylesheet bound exceeded while reading or matching rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    Bytes,
    Rules,
    Declarations,
    Work,
}

impl fmt::Display for Limit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Limit::Bytes => write!(f, "stylesheets exceed {MAX_STYLE_BYTES} bytes"),
            Limit::Rules => write!(f, "more than {MAX_STYLE_RULES} stylesheet rules"),
            Limit::Declarations => {
                write!(
                    f,
                    "more than {MAX_STYLE_DECLARATIONS} stylesheet declarations"
                )
            }
            Limit::Work => write!(f, "stylesheet matching exceeds {MAX_STYLE_WORK} steps"),
        }
    }
}

/// Element class names, linearly searched when few and hashed when many.
enum ClassLookup<'a> {
    Slice(&'a [&'a str]),
    Set(HashSet<&'a str>),
}

impl<'a> ClassLookup<'a> {
    fn new(classes: &'a [&'a str]) -> Self {
        if classes.len() <= 16 {
            ClassLookup::Slice(classes)
        } else {
            ClassLookup::Set(classes.iter().copied().collect())
        }
    }

    fn contains(&self, class: &str) -> bool {
        match self {
            ClassLookup::Slice(slice) => slice.contains(&class),
            ClassLookup::Set(set) => set.contains(class),
        }
    }
}

/// A simple selector: every part given must match.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Selector {
    pub tag: Option<String>,
    pub classes: Vec<String>,
    pub id: Option<String>,
}

impl Selector {
    /// (ids, classes, tags): CSS specificity.
    fn specificity(&self) -> (usize, usize, usize) {
        (
            usize::from(self.id.is_some()),
            self.classes.len(),
            usize::from(self.tag.is_some()),
        )
    }

    fn matches(&self, tag: &str, id: Option<&str>, classes: &ClassLookup<'_>) -> bool {
        self.tag.as_deref().is_none_or(|t| t == tag)
            && self.id.as_deref().is_none_or(|i| Some(i) == id)
            && self.classes.iter().all(|c| classes.contains(c.as_str()))
    }

    /// `tag.class#id` and any order of those; `None` for a selector with a
    /// combinator, a pseudo-class, an attribute test or anything else.
    fn parse(text: &str) -> Option<Selector> {
        let text = text.trim();
        if text.is_empty()
            || text.contains(|c: char| {
                c.is_whitespace() || matches!(c, '>' | '+' | '~' | ':' | '[' | '*' | '(' | ')')
            })
        {
            return None;
        }
        let mut out = Selector::default();
        let mut rest = text;
        while !rest.is_empty() {
            let (kind, body) = match rest.chars().next()? {
                '.' => ('.', &rest[1..]),
                '#' => ('#', &rest[1..]),
                _ => ('t', rest),
            };
            let end = body.find(['.', '#']).unwrap_or(body.len());
            let name = &body[..end];
            if name.is_empty()
                || !name
                    .chars()
                    .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_'))
            {
                return None;
            }
            match kind {
                '.' => {
                    if out.classes.len() >= MAX_SELECTOR_CLASSES {
                        return None;
                    }
                    out.classes.push(name.to_string());
                }
                '#' => {
                    if out.id.is_some() {
                        return None;
                    }
                    out.id = Some(name.to_string());
                }
                _ => {
                    if out.tag.is_some() {
                        return None;
                    }
                    out.tag = Some(name.to_string());
                }
            }
            rest = &body[end..];
        }
        Some(out)
    }
}

/// One rule of a `style` element; declarations live once per block in [`Sheet`].
#[derive(Debug, Clone, PartialEq)]
struct Rule {
    selector: Selector,
    block: usize,
    /// The rule's place in document order, for the cascade among equals.
    order: usize,
}

/// Indexed rules from a document's `<style>` elements. Each rule belongs to
/// exactly one bucket (id, first class, or tag) and shares its declaration
/// block by index with comma-separated siblings.
#[derive(Debug, Default)]
pub struct Sheet {
    blocks: Vec<Vec<(String, String)>>,
    rules: Vec<Rule>,
    by_id: HashMap<String, Vec<usize>>,
    by_class: HashMap<String, Vec<usize>>,
    by_tag: HashMap<String, Vec<usize>>,
    next_order: usize,
    bytes: usize,
    declarations: usize,
}

impl Sheet {
    /// Parse a `<style>` element's text into this sheet; `dropped` records up
    /// to [`MAX_WARNINGS`] skipped selectors or at-rules.
    pub fn parse(&mut self, text: &str, dropped: &mut Vec<String>) -> Result<(), Limit> {
        self.bytes = self.bytes.saturating_add(text.len());
        if self.bytes > MAX_STYLE_BYTES {
            return Err(Limit::Bytes);
        }
        let clean = strip_comments(text);
        let mut rest = clean.as_ref();
        loop {
            rest = rest.trim_start();
            // An at-rule with no block (`@import url(x);`, `@charset`) ends at
            // the first `;` before any `{`; scanning for both at once stays O(N).
            let open = if rest.starts_with('@') {
                match rest.find([';', '{']) {
                    Some(end) if rest.as_bytes()[end] == b';' => {
                        push_dropped(dropped, rest[..end].trim());
                        rest = &rest[end + 1..];
                        continue;
                    }
                    Some(open) => open,
                    None => break,
                }
            } else {
                let Some(open) = rest.find('{') else { break };
                open
            };
            let selectors = rest[..open].trim();
            // An at-rule's block can hold blocks of its own (`@media`,
            // `@keyframes`): it ends at the `}` that closes its `{`.
            let close = if selectors.starts_with('@') {
                block_end(&rest[open..])
            } else {
                rest[open..].find('}')
            };
            let Some(close) = close else {
                break;
            };
            let body = &rest[open + 1..open + close];
            rest = &rest[open + close + 1..];
            // An at-rule (`@import`, `@font-face`, `@media`) is not read: an
            // import is an external reference, and the rest has no place in a
            // picture.
            if selectors.starts_with('@') {
                push_dropped(dropped, selectors);
                continue;
            }
            let mut parsed = Vec::new();
            for part in selectors.split(',') {
                match Selector::parse(part) {
                    Some(selector) => {
                        if self.rules.len() + parsed.len() >= MAX_STYLE_RULES {
                            return Err(Limit::Rules);
                        }
                        parsed.push((selector, self.next_order));
                    }
                    None => push_dropped(dropped, part.trim()),
                }
                self.next_order += 1;
            }
            if parsed.is_empty() {
                continue;
            }
            let decls = parse_declarations(body, &mut self.declarations)?;
            let block = self.blocks.len();
            self.blocks.push(decls);
            for (selector, order) in parsed {
                let index = self.rules.len();
                self.index_rule(&selector, index);
                self.rules.push(Rule {
                    selector,
                    block,
                    order,
                });
            }
        }
        Ok(())
    }

    fn index_rule(&mut self, selector: &Selector, index: usize) {
        if let Some(id) = &selector.id {
            self.by_id.entry(id.clone()).or_default().push(index);
        } else if let Some(class) = selector.classes.first() {
            self.by_class.entry(class.clone()).or_default().push(index);
        } else if let Some(tag) = &selector.tag {
            self.by_tag.entry(tag.clone()).or_default().push(index);
        }
    }

    /// The declarations of every rule matching an element, in cascade order
    /// (specificity, then sheet order), charging candidate checks and matched
    /// declarations against `budget`.
    pub fn matching<'r>(
        &'r self,
        tag: &str,
        id: Option<&str>,
        classes: &[&str],
        budget: &mut usize,
    ) -> Result<Vec<&'r (String, String)>, Limit> {
        if self.rules.is_empty() {
            return Ok(Vec::new());
        }
        let lookup = ClassLookup::new(classes);
        let mut hits: Vec<&Rule> = Vec::new();
        let mut visit = |indices: &[usize], budget: &mut usize| -> Result<(), Limit> {
            for &index in indices {
                let rule = &self.rules[index];
                charge(budget, 1 + rule.selector.classes.len())?;
                if rule.selector.matches(tag, id, &lookup) {
                    hits.push(rule);
                }
            }
            Ok(())
        };
        if let Some(indices) = self.by_tag.get(tag) {
            visit(indices, budget)?;
        }
        if let Some(indices) = id.and_then(|i| self.by_id.get(i)) {
            visit(indices, budget)?;
        }
        if !classes.is_empty() {
            if classes.len() <= 16 {
                for (i, &class) in classes.iter().enumerate() {
                    if classes[..i].contains(&class) {
                        continue;
                    }
                    if let Some(indices) = self.by_class.get(class) {
                        visit(indices, budget)?;
                    }
                }
            } else {
                let mut seen = HashSet::with_capacity(classes.len());
                for &class in classes {
                    if !seen.insert(class) {
                        continue;
                    }
                    if let Some(indices) = self.by_class.get(class) {
                        visit(indices, budget)?;
                    }
                }
            }
        }
        hits.sort_by_key(|r| (r.selector.specificity(), r.order));
        let mut total = 0usize;
        for rule in &hits {
            let len = self.blocks[rule.block].len();
            charge(budget, len)?;
            total += len;
        }
        let mut out = Vec::with_capacity(total);
        for rule in hits {
            out.extend(self.blocks[rule.block].iter());
        }
        Ok(out)
    }
}

fn push_dropped(dropped: &mut Vec<String>, text: &str) {
    if dropped.len() < MAX_WARNINGS {
        dropped.push(text.to_string());
    }
}

fn charge(budget: &mut usize, cost: usize) -> Result<(), Limit> {
    if *budget < cost {
        return Err(Limit::Work);
    }
    *budget -= cost;
    Ok(())
}

fn parse_declarations(text: &str, count: &mut usize) -> Result<Vec<(String, String)>, Limit> {
    let mut out = Vec::new();
    for d in text.split(';') {
        let Some((name, value)) = d.split_once(':') else {
            continue;
        };
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim().trim_end_matches("!important").trim();
        if !name.is_empty() && !value.is_empty() {
            if *count >= MAX_STYLE_DECLARATIONS {
                return Err(Limit::Declarations);
            }
            *count += 1;
            out.push((name, value.to_string()));
        }
    }
    Ok(out)
}

/// `a: b; c: d` as pairs, names lowercased and values trimmed; `!important`
/// is read as the value without it, charging each segment against `budget`.
pub fn declarations_bounded(
    text: &str,
    budget: &mut usize,
) -> Result<Vec<(String, String)>, Limit> {
    let mut out = Vec::new();
    for d in text.split(';') {
        charge(budget, 1)?;
        let Some((name, value)) = d.split_once(':') else {
            continue;
        };
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim().trim_end_matches("!important").trim();
        if !name.is_empty() && !value.is_empty() {
            out.push((name, value.to_string()));
        }
    }
    Ok(out)
}

fn strip_comments(text: &str) -> Cow<'_, str> {
    let Some(first) = text.find("/*") else {
        return Cow::Borrowed(text);
    };
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..first]);
    let mut rest = match text[first + 2..].find("*/") {
        Some(j) => &text[first + 2 + j + 2..],
        None => return Cow::Owned(out),
    };
    while let Some(i) = rest.find("/*") {
        out.push_str(&rest[..i]);
        match rest[i + 2..].find("*/") {
            Some(j) => rest = &rest[i + 2 + j + 2..],
            None => return Cow::Owned(out),
        }
    }
    out.push_str(rest);
    Cow::Owned(out)
}

/// The offset of the `}` closing the block `text` opens with its first `{`.
fn block_end(text: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (i, c) in text.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_exports_classes_cascade_by_specificity_then_order() {
        let mut dropped = Vec::new();
        let mut sheet = Sheet::default();
        sheet
            .parse(
                ".cls-1{fill:#f00;stroke:none}\n/* c */ path{fill:#0f0}\n#mark{fill:#00f !important}\n.cls-1.cls-2{fill:#ff0}\ng > path{fill:#000}\n@import url(x.css);",
                &mut dropped,
            )
            .unwrap();
        assert_eq!(
            dropped,
            vec!["g > path".to_string(), "@import url(x.css)".to_string()]
        );
        let mut budget = MAX_STYLE_WORK;
        let hits = sheet
            .matching("path", Some("mark"), &["cls-1", "cls-2"], &mut budget)
            .unwrap();
        let fills: Vec<&str> = hits
            .iter()
            .filter(|(n, _)| n == "fill")
            .map(|(_, v)| v.as_str())
            .collect();
        assert_eq!(fills, vec!["#0f0", "#f00", "#ff0", "#00f"]);
        assert_eq!(
            sheet
                .matching("rect", None, &["cls-1"], &mut budget)
                .unwrap()
                .len(),
            2
        );
        assert!(sheet
            .matching("rect", None, &[], &mut budget)
            .unwrap()
            .is_empty());
        // Duplicate element classes must not match the same rule twice.
        assert_eq!(
            sheet
                .matching("rect", None, &["cls-1", "cls-1"], &mut budget)
                .unwrap()
                .len(),
            2
        );
    }

    /// An at-rule holding blocks is skipped whole: the rules after it apply.
    #[test]
    fn rules_after_a_nested_at_rule_are_read() {
        let mut dropped = Vec::new();
        let mut sheet = Sheet::default();
        sheet
            .parse(
                "@keyframes spin { from { opacity: 1 } to { opacity: 0 } } .cls { fill: #c00 }\n\
                 @media (min-width: 1px) { .cls { fill: #0c0 } } rect { stroke: #00c }",
                &mut dropped,
            )
            .unwrap();
        assert_eq!(
            dropped,
            vec![
                "@keyframes spin".to_string(),
                "@media (min-width: 1px)".to_string()
            ]
        );
        let mut budget = MAX_STYLE_WORK;
        let hits: Vec<_> = sheet
            .matching("rect", None, &["cls"], &mut budget)
            .unwrap()
            .into_iter()
            .cloned()
            .collect();
        assert_eq!(
            hits,
            vec![
                ("stroke".to_string(), "#00c".to_string()),
                ("fill".to_string(), "#c00".to_string())
            ]
        );
    }

    #[test]
    fn a_style_attribute_splits_into_declarations() {
        let mut budget = MAX_STYLE_WORK;
        assert_eq!(
            declarations_bounded("fill:red; stroke-width: 2 ;;bad;", &mut budget).unwrap(),
            vec![
                ("fill".to_string(), "red".to_string()),
                ("stroke-width".to_string(), "2".to_string())
            ]
        );
    }

    #[test]
    fn adversarial_stylesheets_are_bounded_without_oom_or_quadratic_copies() {
        // 1. Selector × declaration fan-out (~100 KB) refuses at rule cap without cloning blocks.
        let selectors = vec!["a"; 25_000].join(",");
        let decls = "f:1;".repeat(12_500);
        let fanout = format!("{selectors}{{{decls}}}");
        let mut dropped = Vec::new();
        assert_eq!(
            Sheet::default().parse(&fanout, &mut dropped),
            Err(Limit::Rules)
        );

        // 2. Shared block across many selectors within the cap stores declarations once and
        //    bounds matching work when an element matches all of them.
        let selectors = vec!["rect"; 2_000].join(",");
        let decls = "fill:red;".repeat(1_000);
        let shared = format!("{selectors}{{{decls}}}");
        let mut sheet = Sheet::default();
        sheet.parse(&shared, &mut dropped).unwrap();
        assert_eq!(sheet.blocks.len(), 1);
        assert_eq!(sheet.blocks[0].len(), 1_000);
        let mut budget = MAX_STYLE_WORK;
        assert_eq!(
            sheet.matching("rect", None, &[], &mut budget),
            Err(Limit::Work)
        );

        // 3. ~1 MiB of `a{}` rules refuses at rule cap in linear time.
        let many_rules = "a{}".repeat(349_000);
        assert_eq!(
            Sheet::default().parse(&many_rules, &mut dropped),
            Err(Limit::Rules)
        );

        // 4. Large stream of `;`-terminated at-rules parses linearly and caps dropped warnings.
        let many_at = "@a;".repeat(100_000);
        let mut dropped = Vec::new();
        Sheet::default().parse(&many_at, &mut dropped).unwrap();
        assert_eq!(dropped.len(), MAX_WARNINGS);
    }
}
