//! SVG attribute lengths use the kernel's units, lists and marker keywords.

use exact_kernel::svg::length::{AttributeLength, Length, ATTRIBUTE_LENGTH_DIAGNOSTIC};
use exact_kernel::PropId;
use exact_runner::bridge::BridgeError;

pub fn refused(error: &BridgeError) -> bool {
    matches!(error, BridgeError::PropKind { prop, .. } if AttributeLength::of(*prop).is_some())
}

pub fn binding(
    uses: &mut crate::code::Uses,
    prop: PropId,
    value: &str,
    element: &str,
) -> Option<String> {
    let grammar = AttributeLength::of(prop)?;
    let parse = uses.constant("$svgLength", parser);
    let say = uses.rt("say");
    let view = uses.rt("viewId");
    let keywords = serde_json::to_string(grammar.keywords).unwrap();
    let list = grammar.list;
    let name = prop.name();
    let map = uses.share('m', format!(
        r#"(v,id)=>{{if({parse}(v,{list},{keywords}))return v;{say}(`{ATTRIBUTE_LENGTH_DIAGNOSTIC}: view ${{id}}: invalid {name} value ${{JSON.stringify(v)}}; unset`);return null}}"#
    ));
    Some(format!("()=>{map}(({value})(),{view}({element}))"))
}

fn parser() -> String {
    let units = Length::ABSOLUTE_UNITS
        .iter()
        .filter_map(|(unit, _)| (!unit.is_empty()).then_some(*unit))
        .collect::<Vec<_>>()
        .join("|");
    let separators = (0u8..=127)
        .filter(|c| *c == b',' || c.is_ascii_whitespace())
        .map(|c| format!(r"\x{c:02x}"))
        .collect::<String>();
    // Rust str::trim's White_Space set; JavaScript \s also includes BOM and omits NEL.
    let space = r"\x09-\x0d \x85\xa0\u1680\u2000-\u200a\u2028\u2029\u202f\u205f\u3000";
    let number = r"[+-]?(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:e[+-]?[0-9]+)?";
    format!(
        r#"(v,list,keywords)=>{{const trim=s=>s.replace(/^[{space}]+|[{space}]+$/g,"");const t=String(v);if(keywords.includes(trim(t)))return true;const read=s=>{{const m=/^({number})[{space}]*(?:{units}|%)?$/i.exec(trim(s));return m!==null&&Number.isFinite(Math.fround(Number(m[1])))}};if(!list)return read(t);const words=t.split(/[{separators}]/).filter(Boolean);return words.length>0&&words.every(read)}}"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computed_attribute_lengths_agree_with_the_kernel() {
        let values = [
            "0",
            "-2",
            "+.5",
            "1.",
            "1e2",
            "-1.5E-2PX",
            "1px",
            "1in",
            "1cm",
            "1mm",
            "1pt",
            "1pc",
            "50%",
            "-20%",
            "1e-1000",
            "3.4028235e38",
            "1e39",
            "1e9999",
            "1 px",
            "1 %",
            " 1px ",
            "\u{85}1px\u{85}",
            "\u{feff}1px",
            "1 2px, 3%",
            "10 20 \u{a0}",
            "\u{a0} 10 20",
            "10 20\u{a0}",
            "\u{a0}10 20",
            "1,,2",
            "1px,2em",
            "1px\t2in",
            "1\u{b}2",
            "1\u{b}px",
            "1\u{c}2",
            "1\r2",
            "1\n2",
            "center",
            "left",
            "right",
            "top",
            "bottom",
            "CENTER",
            "",
            " ",
            ",",
            "px",
            "NaN",
            "inf",
            "1e",
            "1e+",
            "1e2e3",
            "2em",
            "2EX",
            "2rem",
            "2ch",
            "2vh",
            "2vw",
            "2dvh",
            "1q",
            "1px junk",
            "calc(1px + 2px)",
            "env(safe-area-inset-top)",
            "var(--x)",
            "min(1px,2px)",
        ];
        let mut cases = Vec::new();
        for prop in [PropId::X1, PropId::TextX, PropId::RefX, PropId::RefY] {
            let grammar = AttributeLength::of(prop).unwrap();
            for value in values {
                cases.push(serde_json::json!([
                    value,
                    grammar.list,
                    grammar.keywords,
                    grammar.accepts(value)
                ]));
            }
        }
        let script = format!(
            "const parse={};for(const [v,list,keywords,expected] of {}){{const actual=parse(v,list,keywords);if(actual!==expected)throw Error(JSON.stringify({{v,list,keywords,actual,expected}}));}}",
            parser(), serde_json::to_string(&cases).unwrap()
        );
        let output =
            std::process::Command::new(std::env::var_os("BUN").unwrap_or_else(|| "bun".into()))
                .args(["-e", &script])
                .output()
                .expect("bun");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
