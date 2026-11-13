//! @ref LLP 1121 D10 (stage 6 F) — an icon token's SVG, read at build: for
//! v1 one monochrome shape, its `<path>`s each filled under its own viewBox,
//! as a template, as the roles' paths are drawn; their `d`s travel joined by
//! `;`, which no path data holds. Each is filled nonzero, or (stage 14 C2)
//! even-odd where its `fill-rule` says so: one rule for the whole icon, so
//! paths that mix rules are no template.
//! Anything else (several colours, a gradient, an image, text, a stroke, a
//! transform, a shape that is not a path) is no template: the web may draw
//! it as the file, and a shipped target that is not the web refuses it.

/// An SVG read as an icon.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Svg {
    /// Its viewBox, as written (`0 -960 960 960`).
    pub view_box: String,
    /// Its paths' `d`, joined by `;`.
    pub path: String,
    /// Its paths are filled even-odd (every one: [`read`] refuses a mix),
    /// else nonzero.
    pub even_odd: bool,
    /// Why it is no one-colour shape, when it is not.
    pub not_template: Option<String>,
    /// What to change in the file so that it is one, beside `not_template`.
    pub fix: String,
}

/// The fix for a property a template loses.
fn fix_lost(property: &str) -> &'static str {
    match property {
        "transform" => "apply the transform to the path's coordinates and remove it (an editor's Object to Path, or SVGO's `convertTransform`, does)",
        "fill-rule" => "write `nonzero` or `evenodd`, the same on every path: a template fills its paths by one rule (for nonzero, a hole is drawn in the opposite direction to its outline; an editor's path union or Combine makes it so)",
        "opacity" | "fill-opacity" => "remove it: the host paints the shape opaque, in the text's colour",
        "display" | "visibility" => "delete the hidden element",
        _ => "remove it, cutting the shape itself instead (an editor's Intersect or Difference)",
    }
}

/// The fix for an element that is not a path.
const FIX_SHAPE: &str = "convert it to a `<path>` (an editor's Object to Path; `<rect x=\"2\" y=\"2\" width=\"20\" height=\"20\"/>` is `<path d=\"M2 2h20v20H2Z\"/>`)";

/// What an element passes its children: its fill, stroke, `color` and
/// `fill-rule`, and whether it is inside `<defs>` (or a mask, a pattern …).
type Open = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    bool,
);

/// The fix for a template's `style` (C2, closed): a template reads only
/// its presentation attributes.
const FIX_STYLE: &str = "write the paint as attributes (`fill=\"currentColor\"`, `fill-rule=\"evenodd\"`) and drop the `style` attribute or the `<style>` block: a template reads neither";

/// The fix for XML a template is not (a namespace, a CDATA section, a DOCTYPE).
const FIX_PLAIN: &str = "remove it: a template is plain SVG, its elements and attributes unprefixed in the root's default namespace (`xmlns=\"http://www.w3.org/2000/svg\"`), with no CDATA or DOCTYPE";

/// The fix for a presentation value a template doesn't read.
const FIX_VALUE: &str = "write `fill` as `currentColor`, `none`, `#rgb` or `#rrggbb`, and `fill-rule` as `nonzero` or `evenodd`, exactly";

/// The fix for `color`: what `currentColor` paints is the host's.
const FIX_COLOR: &str =
    "remove `color`: a template is drawn in the host's colour; write `fill=\"currentColor\"`";

/// A presentation attribute a template reads, and its value refused unless
/// it is one of the few a template takes, exactly (C2, closed): `fill` is
/// `currentColor`, `none`, `#rgb` or `#rrggbb`; `fill-rule` is `nonzero` or
/// `evenodd`; case as before, ASCII whitespace trimmed, no other character,
/// keyword (`inherit`) or function. `color` is refused whatever it says. The
/// survey behind the list: the repo's template SVGs write `fill` as
/// `currentColor` or `none` (and an avatar picture `#999`), and no
/// `fill-rule`, `color` or `black`.
fn attribute_refused(attrs: &[(&str, String)]) -> Option<(String, &'static str)> {
    attrs.iter().find_map(|(k, v)| {
        let value = v.trim_matches(|c: char| c.is_ascii_whitespace());
        let lower = value.to_ascii_lowercase();
        let hex = |h: &str| matches!(h.len(), 3 | 6) && h.chars().all(|c| c.is_ascii_hexdigit());
        let read = match *k {
            "color" => return Some((format!("`color=\"{v}\"`"), FIX_COLOR)),
            "fill" => {
                value.is_ascii()
                    && (matches!(lower.as_str(), "currentcolor" | "none")
                        || lower.strip_prefix('#').is_some_and(hex))
            }
            "fill-rule" => value.is_ascii() && matches!(lower.as_str(), "nonzero" | "evenodd"),
            _ => true,
        };
        (!read).then(|| {
            (
                format!("`{k}=\"{v}\"`, not a value a template reads"),
                FIX_VALUE,
            )
        })
    })
}

/// A property's value on an element: its `style` declaration first, else
/// its presentation attribute (SVG's cascade). A declaration's
/// `!important` is CSS's; a presentation attribute takes no `!important`
/// (SVG reads `fill-rule="evenodd !important"` as invalid, C2 round 1,
/// Astra 6), so its value is kept whole.
fn property(attrs: &[(&str, String)], name: &str) -> Option<String> {
    // The last declaration wins, an `!important` one over any other.
    let styled = attrs
        .iter()
        .find(|(k, _)| *k == "style")
        .and_then(|(_, s)| {
            let declared = s.split(';').filter_map(|d| {
                let (k, v) = d.split_once(':')?;
                // A property's name in any case, as CSS reads it (round 2, R2-5).
                (k.trim().eq_ignore_ascii_case(name)).then(|| (importance(v).0, clean(v)))
            });
            declared.fold(
                None,
                |won: Option<(bool, String)>, (important, v)| match won {
                    Some((true, _)) if !important => won,
                    _ => Some((important, v)),
                },
            )
        });
    let styled = styled.map(|(_, v)| v);
    styled.or_else(|| {
        attrs
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.trim().to_ascii_lowercase())
    })
}

/// A paint's value, lowercase, without `!important`.
fn clean(v: &str) -> String {
    importance(v).1
}

/// Whether a declared value is `!important`, in any case and with space
/// after the `!` as CSS reads it (round 3, R3-2), and the value lowercase
/// without it.
fn importance(v: &str) -> (bool, String) {
    let v = v.trim().to_ascii_lowercase();
    let stripped = v
        .strip_suffix("important")
        .map(str::trim_end)
        .and_then(|rest| rest.strip_suffix('!'));
    match stripped {
        Some(rest) => (true, rest.trim().to_string()),
        None => (false, v),
    }
}

/// Whether `d` is path data alone, so it travels in the source form.
fn path_data(d: &str) -> bool {
    d.chars().all(|c| {
        c.is_ascii_digit() || c.is_ascii_whitespace() || "MmZzLlHhVvCcSsQqTtAaEe.,+-".contains(c)
    })
}

/// Each reason an SVG is no template, with its fix.
type Why = Vec<(String, &'static str)>;

/// Add a reason to `why`, once.
fn note(why: &mut Why, reason: impl FnOnce() -> (String, &'static str)) {
    let (said, fix) = reason();
    if !why.iter().any(|(r, _)| *r == said) {
        why.push((said, fix));
    }
}

/// Read `text` as an icon's SVG, or why it is none.
pub(crate) fn read(text: &str) -> Result<Svg, String> {
    // Every reason it is no template, so one build names each fix.
    let (mut view_box, mut paths, mut fill, mut why) =
        (None, Vec::<String>::new(), None::<String>, Why::new());
    // Each path's fill rule after the cascade, in order: `true` even-odd.
    let mut rules: Vec<bool> = Vec::new();
    let mut stack: Vec<Open> = Vec::new();
    let tokens = exact_svg_file::xml::tokenize(text).map_err(|e| e.to_string())?;
    if tokens.has_cdata {
        note(&mut why, || {
            ("it has a CDATA section".to_string(), FIX_PLAIN)
        });
    }
    if tokens.has_doctype {
        note(&mut why, || ("it has a DOCTYPE".to_string(), FIX_PLAIN));
    }
    for event in tokens.events {
        use exact_svg_file::xml::Event;
        let (name, attrs, empty) = match &event {
            Event::Start {
                name, attrs, empty, ..
            } => (name.as_str(), attrs, *empty),
            Event::End { .. } => {
                stack.pop();
                continue;
            }
            Event::Text(_) => continue,
        };
        let attrs: Vec<(&str, String)> =
            attrs.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
        // Namespaces (C2, the last gaps): a prefixed element or attribute,
        // or any namespace declaration but the root's default SVG one, is no
        // template (`<s:style xmlns:s=…>`, `<g xmlns="…xhtml">`). No repo
        // template uses another (none writes `xlink`).
        if name.contains(':') {
            note(&mut why, || {
                (format!("its `<{name}>` has a namespace prefix"), FIX_PLAIN)
            });
        }
        for (k, v) in &attrs {
            let root_svg = stack.is_empty()
                && name == "svg"
                && *k == "xmlns"
                && v == "http://www.w3.org/2000/svg";
            if (*k == "xmlns" || k.starts_with("xmlns:")) && !root_svg {
                note(&mut why, || {
                    (format!("its `<{name}>` declares `{k}=\"{v}\"`"), FIX_PLAIN)
                });
            } else if k.contains(':') && !k.starts_with("xmlns:") {
                note(&mut why, || {
                    (
                        format!("its `<{name}>` has `{k}`, a namespaced attribute"),
                        FIX_PLAIN,
                    )
                });
            }
        }
        let (inherited_fill, inherited_stroke, inherited_color, inherited_rule, in_defs) = stack
            .last()
            .cloned()
            .unwrap_or((None, None, None, None, false));
        let fill_here = property(&attrs, "fill").or(inherited_fill);
        let stroke_here = property(&attrs, "stroke").or(inherited_stroke);
        // What `currentColor` is here: the nearest `color` (round 3, R3-1);
        // `inherit` and `currentColor` keep the parent's (re-check, RE-2).
        let color_here = property(&attrs, "color")
            .filter(|c| c != "inherit" && c != "currentcolor")
            .or(inherited_color);
        // Its fill rule (stage 14 C2): its own, or its parent's (`inherit`
        // keeps the parent's, as CSS reads it); nonzero at the root.
        let rule_here = property(&attrs, "fill-rule")
            .filter(|r| r != "inherit")
            .or(inherited_rule);
        let in_defs =
            in_defs || matches!(name, "defs" | "symbol" | "clipPath" | "mask" | "pattern");
        // A `<style>` block, or a `style` attribute holding more than plain
        // declarations, anywhere (in `<defs>` too: an exporter's class
        // rules live there), is no template (C2 round 2).
        if name == "style" {
            note(&mut why, || {
                ("it has a `<style>` block".to_string(), FIX_STYLE)
            });
        }
        // Any `style` attribute, anywhere: a template reads only its
        // presentation attributes (C2, closed).
        if attrs.iter().any(|(k, _)| *k == "style") {
            note(&mut why, || {
                (format!("its `<{name}>` has a `style`"), FIX_STYLE)
            });
        }
        // A presentation value a template reads, exactly.
        if let Some((what, fix)) = (!in_defs).then(|| attribute_refused(&attrs)).flatten() {
            note(&mut why, || (format!("its `<{name}>` has {what}"), fix));
        }
        // What a template drawn filled, by one rule, at full opacity would lose.
        if !in_defs {
            let lost = [
                "transform",
                "fill-rule",
                "opacity",
                "fill-opacity",
                "clip-path",
                "mask",
                "filter",
            ]
            .into_iter()
            .chain(["display", "visibility"])
            .find(|p| {
                property(&attrs, p).is_some_and(|v| match *p {
                    // Either rule, or the parent's (stage 14 C2); no other value.
                    "fill-rule" => !matches!(v.as_str(), "nonzero" | "evenodd" | "inherit"),
                    "opacity" | "fill-opacity" => v.parse::<f64>().map_or(true, |o| o < 1.0),
                    "display" => v == "none",
                    "visibility" => v != "visible",
                    _ => v != "none",
                })
            });
            if let Some(p) = lost {
                note(&mut why, || {
                    (format!("its `<{name}>` has a `{p}`"), fix_lost(p))
                });
            }
        }
        match name {
            // A nested viewport places and scales what it holds.
            "svg" if !stack.is_empty() => {
                note(&mut why, || {
                    (
                        "it nests an `<svg>`".to_string(),
                        "move the inner `<svg>`'s paths into the outer one, in the outer viewBox's coordinates",
                    )
                });
            }
            "svg" => {
                let attr = |a: &str| attrs.iter().find(|(k, _)| *k == a).map(|(_, v)| v.clone());
                view_box = attr("viewBox").or_else(|| {
                    let n = |a| attr(a).map(|v: String| v.trim_end_matches("px").to_string());
                    Some(format!("0 0 {} {}", n("width")?, n("height")?))
                });
            }
            "path" if !in_defs => {
                let d = attrs
                    .iter()
                    .find(|(k, _)| *k == "d")
                    .map(|(_, v)| v.trim().to_string())
                    .filter(|d| !d.is_empty())
                    .ok_or("a `<path>` has no `d`")?;
                if !path_data(&d) {
                    return Err("a `<path>`'s `d` is not path data".into());
                }
                let painted = fill_here.clone().unwrap_or_else(|| "black".into());
                // `currentColor` paints the element's `color` (round 3,
                // R3-1): two under two colours are two, one under red is
                // red as an explicit red fill is; with no `color` set, the
                // host's colour, as on main.
                let painted = match (painted.as_str(), &color_here) {
                    ("currentcolor", Some(color)) => color.clone(),
                    _ => painted,
                };
                if painted.starts_with("url(") {
                    note(&mut why, || {
                        (
                            "it fills with a gradient or a pattern".to_string(),
                            "fill every path with `fill=\"currentColor\"`: the host paints it in the text's colour",
                        )
                    });
                }
                // A colour with its own alpha (`rgba()`, `#rrggbbaa`) is no opaque template.
                let hex = painted.strip_prefix('#').map_or(0, str::len);
                if painted.contains('/')
                    || painted.starts_with("rgba")
                    || painted.starts_with("hsla")
                    || painted == "transparent"
                    || hex == 4
                    || hex == 8
                {
                    note(&mut why, || {
                        (
                            "it fills in a translucent colour".to_string(),
                            "fill with `fill=\"currentColor\"`: the host paints it in the text's colour, opaque",
                        )
                    });
                }
                // An outline icon: its lines are strokes, which a template does not draw.
                let outline = "outline the stroke into a filled path and remove the `stroke` (Inkscape's Path > Stroke to Path, Figma's and Illustrator's Outline Stroke)";
                if stroke_here.as_deref().is_some_and(|s| s != "none") {
                    note(&mut why, || ("it has a stroke".to_string(), outline));
                }
                if painted == "none" {
                    note(&mut why, || ("a path is not filled".to_string(), outline));
                } else if fill.as_ref().is_some_and(|f| *f != painted) {
                    note(&mut why, || {
                        (
                            "it fills in more than one colour".to_string(),
                            "fill every path in the same colour: the host paints the whole shape in one",
                        )
                    });
                } else {
                    fill = Some(painted);
                }
                paths.push(d);
                rules.push(rule_here.as_deref() == Some("evenodd"));
            }
            "path" | "g" | "title" | "desc" | "defs" | "metadata" | "style" => {}
            _ if in_defs => {}
            other => {
                note(&mut why, || {
                    (format!("it has a `<{other}>`, not only paths"), FIX_SHAPE)
                });
            }
        }
        if !empty {
            stack.push((fill_here, stroke_here, color_here, rule_here, in_defs));
        }
    }
    let view_box = view_box.ok_or("it has no `<svg>` with a viewBox (or a width and height)")?;
    let numbers: Vec<f64> = view_box
        .split([' ', ','])
        .filter(|s| !s.is_empty())
        .map(str::parse)
        .collect::<Result<_, _>>()
        .map_err(|_| format!("its viewBox `{view_box}` is not four numbers"))?;
    if numbers.len() != 4 || numbers[2] <= 0.0 || numbers[3] <= 0.0 {
        return Err(format!(
            "its viewBox `{view_box}` is not four numbers, a positive size"
        ));
    }
    if paths.is_empty() && why.is_empty() {
        return Err("it has no `<path>`".into());
    }
    // One rule for the icon (stage 14 C2, theme2's Q1): paths that mix them
    // are named, by their place in the file.
    let even_odd = rules.first().copied().unwrap_or(false);
    if rules.iter().any(|r| *r != even_odd) {
        let named = |odd: bool| {
            rules
                .iter()
                .enumerate()
                .filter(|(_, r)| **r == odd)
                .map(|(i, _)| (i + 1).to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };
        let (odd, nonzero) = (named(true), named(false));
        // Numbered among the drawn paths (those not in `<defs>`, C2 round 1).
        note(&mut why, || {
            (
                format!("its paths mix fill rules (evenodd: drawn path {odd}; nonzero: drawn path {nonzero})"),
                "give every path the same `fill-rule`, or none (nonzero): a template fills its paths by one rule",
            )
        });
    }
    let mut fixes: Vec<&str> = Vec::new();
    for (_, fix) in &why {
        if !fixes.contains(fix) {
            fixes.push(fix);
        }
    }
    Ok(Svg {
        view_box: numbers
            .iter()
            .map(|n| exact_num::Shortest(*n).to_string())
            .collect::<Vec<_>>()
            .join(" "),
        path: paths.join(";"),
        even_odd,
        fix: fixes.join("; "),
        not_template: (!why.is_empty()).then(|| {
            why.into_iter()
                .map(|(r, _)| r)
                .collect::<Vec<_>>()
                .join("; ")
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::read;

    #[test]
    fn malformed_xml_and_declared_entities_are_refused() {
        for (svg, reason) in [
            (
                "<svg viewBox='0 0 24 24'>\n<g><path d='M0 0h1'/></svg>",
                "closes `<g>`",
            ),
            (
                "<svg viewBox='0 0 24 24'><path d='M0 0h1'/>",
                "never closed",
            ),
            (
                "<!DOCTYPE svg [<!ENTITY a 'x'>]><svg viewBox='0 0 24 24'><path d='M0 0h1'/></svg>",
                "entity declaration",
            ),
            (
                "<svg viewBox='0 0 24 24' aria-label='&missing;'><path d='M0 0h1'/></svg>",
                "&missing;",
            ),
        ] {
            let error = read(svg).expect_err("malformed SVG must be refused");
            assert!(error.contains(reason), "{error}");
        }
    }

    #[test]
    fn a_material_symbol_is_one_path_under_its_viewbox() {
        let svg = read(r#"<svg xmlns="http://www.w3.org/2000/svg" height="24" viewBox="0 -960 960 960" width="24"><path d="M280-80 120-240Z"/></svg>"#).unwrap();
        assert_eq!(svg.view_box, "0 -960 960 960");
        assert_eq!(svg.path, "M280-80 120-240Z");
        assert_eq!(svg.not_template, None);
    }

    #[test]
    fn several_paths_of_one_fill_join_and_others_are_no_template() {
        let svg = read(r##"<svg viewBox="0 0 24 24"><g fill="#000"><path d="M0 0h1"/><path d="m2 2h1"/></g></svg>"##).unwrap();
        assert_eq!(svg.path, "M0 0h1;m2 2h1");
        assert_eq!(svg.not_template, None);
        let two = read(r##"<svg viewBox="0 0 24 24"><path fill="#f00" d="M0 0h1"/><path fill="#0000ff" d="M2 2h1"/></svg>"##).unwrap();
        assert_eq!(
            two.not_template.as_deref(),
            Some("it fills in more than one colour")
        );
        let gradient =
            read(r#"<svg viewBox="0 0 24 24"><linearGradient id="a"/><path d="M0 0h1"/></svg>"#)
                .unwrap();
        assert!(gradient.not_template.unwrap().contains("linearGradient"));
        // One colour is a template: `currentColor`, a hex fill, the default.
        for svg in [
            r#"<svg viewBox="0 0 24 24"><path fill="currentColor" d="M0 0h1"/><path fill="CurrentColor" d="M2 2h1"/></svg>"#,
            r##"<svg viewBox="0 0 24 24"><path fill="#000" d="M0 0h1"/><path fill="#000" d="M2 2h1"/></svg>"##,
            r#"<svg viewBox="0 0 24 24" fill="currentColor"><path d="M0 0h4v4H0Z"/></svg>"#,
            r#"<svg viewBox="0 0 24 24"><path d="M0 0h1"/></svg>"#,
        ] {
            assert_eq!(read(svg).unwrap().not_template, None, "{svg}");
        }
        // `color` is refused whatever it says (C2, closed): a template is
        // drawn in the host's colour.
        let color = read(
            r#"<svg viewBox="0 0 24 24" color="red"><path fill="currentColor" d="M0 0h1"/></svg>"#,
        )
        .unwrap();
        assert_eq!(
            color.not_template.as_deref(),
            Some("its `<svg>` has `color=\"red\"`")
        );
        assert!(
            color.fix.contains(
                "a template is drawn in the host's colour; write `fill=\"currentColor\"`"
            ),
            "{}",
            color.fix
        );
        assert!(read(r#"<svg viewBox="0 0 0 24"><path d="M0 0"/></svg>"#).is_err());
        assert!(read(r#"<svg><path d="M0 0"/></svg>"#).is_err());
    }

    /// An outline icon (Lucide's shape: strokes, a `<rect>`) names each reason
    /// once and each fix, so one build says everything to change.
    #[test]
    fn every_reason_is_named_with_its_fix() {
        let svg = read(r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor"><path d="M12 20h9"/><path d="M3 3h1"/><rect x="2" y="2" width="4" height="4"/></svg>"#).unwrap();
        assert_eq!(
            svg.not_template.as_deref(),
            Some("it has a stroke; a path is not filled; it has a `<rect>`, not only paths")
        );
        assert!(svg.fix.contains("Stroke to Path") && svg.fix.contains("Object to Path"));
        assert_eq!(svg.fix.matches("Stroke to Path").count(), 1);
    }

    /// Review round 1: comments and CDATA draw nothing; an inherited fill is
    /// the path's; a transform, a stroke and an unfilled path are no
    /// template; `|` is no path data; `defs` draw nothing; entities are
    /// decoded; attributes are apart.
    #[test]
    fn the_svg_is_read_as_xml_and_its_cascade() {
        let commented = read(r#"<svg viewBox="0 0 24 24"><!-- > <path d="M9 9"/> --><![CDATA[<path d="M8 8"/>]]><path d="M0 0h1"/></svg>"#).unwrap();
        assert_eq!(commented.path, "M0 0h1");
        let gradient = read(r#"<svg viewBox="0 0 24 24"><defs><linearGradient id="g"/></defs><path fill="url(#g)" d="M0 0h1"/></svg>"#).unwrap();
        assert!(gradient.not_template.is_some());
        for hidden in [
            r#"display="none""#,
            r#"visibility="hidden""#,
            r##"fill="#ff000080""##,
            r#"fill="rgba(0,0,0,.5)""#,
            r#"fill="transparent""#,
            r#"opacity="0.5""#,
        ] {
            let svg = read(&format!(
                r#"<svg viewBox="0 0 24 24"><path {hidden} d="M0 0h1"/></svg>"#
            ))
            .unwrap();
            assert!(svg.not_template.is_some(), "{hidden}");
        }
        let nested = read(r#"<svg viewBox="0 0 24 24"><svg x="12" viewBox="0 0 10 10"><path d="M0 0h1"/></svg></svg>"#).unwrap();
        assert!(nested.not_template.is_some());
        let nested = read(r##"<svg viewBox="0 0 24 24"><g fill="#f00"><g fill="#00f"><path d="M0 0h1"/><path d="M2 2h1"/></g></g></svg>"##).unwrap();
        assert_eq!(nested.not_template, None);
        let moved =
            read(r#"<svg viewBox="0 0 24 24"><path transform="scale(2)" d="M0 0h1"/></svg>"#)
                .unwrap();
        assert!(moved.not_template.unwrap().contains("transform"));
        for lost in [r#"fill-rule="winding""#, r#"clip-path="url(#a)""#] {
            let svg = read(&format!(
                r#"<svg viewBox="0 0 24 24"><path {lost} d="M0 0h1"/></svg>"#
            ))
            .unwrap();
            assert!(svg.not_template.is_some(), "{lost}");
        }
        let fine = read(
            r#"<svg viewBox="0 0 24 24"><path fill-rule="nonzero" opacity="1" d="M0 0h1"/></svg>"#,
        )
        .unwrap();
        assert_eq!(fine.not_template, None);
        let quoted =
            read(r#"<svg aria-label="A > B" viewBox="0 0 24 24"><path d="M0 0h1"/></svg>"#)
                .unwrap();
        assert_eq!(quoted.path, "M0 0h1");
        assert!(read(r#"<svg viewBox="0 0 24 24"><path d="M0 0|L24 24"/></svg>"#).is_err());
        let defs = read(
            r#"<svg viewBox="0 0 24 24"><defs><path d="M9 9h1"/></defs><path d="M0 0h1"/></svg>"#,
        )
        .unwrap();
        assert_eq!(defs.path, "M0 0h1");
        let entity = read(r#"<svg viewBox="0&#32;0 24 24"><path d="M0&#x20;0h1"/></svg>"#).unwrap();
        assert_eq!(
            (entity.view_box.as_str(), entity.path.as_str()),
            ("0 0 24 24", "M0 0h1")
        );
        assert!(read(r#"<svg viewBox="0 0 24 24"><path d="M0 0"fill="red"/></svg>"#).is_err());
    }

    #[test]
    fn numeric_references_may_have_leading_zeroes() {
        for zero in ["&#00000000000048;", "&#x00000000000030;"] {
            let svg = read(&format!(
                r#"<svg viewBox="0 0 24 24"><path d="M{zero} 0h1"/></svg>"#
            ))
            .unwrap();
            assert_eq!(svg.not_template, None);
            assert_eq!(svg.path, "M0 0h1");
        }
        for invalid in ["&#0000001114112;", "&#x00000000d800;", "&unknown;"] {
            assert!(read(&format!(
                r#"<svg viewBox="0 0 24 24"><path d="M{invalid} 0h1"/></svg>"#
            ))
            .is_err());
        }
    }

    /// Stage 14 C2: a path's `fill-rule` from its attribute or inherited from
    /// `<g>` and `<svg>`; even-odd is a template, one rule for the icon; a mix
    /// is refused naming the drawn paths.
    #[test]
    fn a_fill_rule_is_read_through_the_cascade_one_for_the_icon() {
        for svg in [
            r#"<svg viewBox="0 0 24 24"><path fill-rule="evenodd" d="M0 0h1"/></svg>"#,
            r#"<svg viewBox="0 0 24 24"><path fill-rule=" EvenOdd " d="M0 0h1"/></svg>"#,
            r#"<svg viewBox="0 0 24 24"><g fill-rule="evenodd"><path d="M0 0h1"/><path d="M2 2h1"/></g></svg>"#,
            r#"<svg viewBox="0 0 24 24" fill-rule="evenodd"><g><path d="M0 0h1"/></g></svg>"#,
        ] {
            let read = read(svg).unwrap();
            assert_eq!(
                (read.not_template.as_deref(), read.even_odd),
                (None, true),
                "{svg}"
            );
        }
        for svg in [
            r#"<svg viewBox="0 0 24 24"><path d="M0 0h1"/></svg>"#,
            r#"<svg viewBox="0 0 24 24"><path fill-rule="nonzero" d="M0 0h1"/></svg>"#,
            r#"<svg viewBox="0 0 24 24"><g fill-rule="evenodd"><path fill-rule="nonzero" d="M0 0h1"/></g></svg>"#,
        ] {
            let read = read(svg).unwrap();
            assert_eq!(
                (read.not_template.as_deref(), read.even_odd),
                (None, false),
                "{svg}"
            );
        }
        let mixed = read(r#"<svg viewBox="0 0 24 24"><path fill-rule="evenodd" d="M0 0h1"/><path d="M2 2h1"/><g fill-rule="evenodd"><path d="M4 4h1"/></g></svg>"#).unwrap();
        assert_eq!(
            mixed.not_template.as_deref(),
            Some("its paths mix fill rules (evenodd: drawn path 1, 3; nonzero: drawn path 2)")
        );
        assert!(mixed.fix.contains("the same `fill-rule`"), "{}", mixed.fix);
        // A rule written in `<defs>` paints nothing; `clip-rule` outside a
        // `<clipPath>` paints nothing either.
        let defs = read(r#"<svg viewBox="0 0 24 24"><defs><path fill-rule="evenodd" d="M9 9h1"/></defs><path clip-rule="evenodd" d="M0 0h1"/></svg>"#).unwrap();
        assert_eq!((defs.not_template, defs.even_odd), (None, false));
        // The other reasons are refused as before, beside an even-odd rule.
        for lost in [
            r#"transform="scale(2)""#,
            r#"opacity="0.5""#,
            r#"stroke="black""#,
            r#"fill="none""#,
            r#"clip-path="url(#a)""#,
            r#"mask="url(#a)""#,
            r#"filter="url(#a)""#,
        ] {
            let svg = read(&format!(
                r#"<svg viewBox="0 0 24 24"><path fill-rule="evenodd" {lost} d="M0 0h1"/></svg>"#
            ))
            .unwrap();
            assert!(svg.not_template.is_some(), "{lost}");
        }
    }

    /// C2, closed (theme2): a template reads no `style` at all and its
    /// presentation values exactly. Every bypass from every round is refused,
    /// each naming what it found.
    #[test]
    fn every_bypass_from_every_round_is_refused() {
        let cases = [
            // Round 1: the comment override; attribute `!important`.
            (
                r#"<path fill-rule="evenodd" style="/* x */ fill-rule:nonzero" d="M0 0h1"/>"#,
                "its `<path>` has a `style`",
            ),
            (
                r#"<path fill-rule="evenodd !important" d="M0 0h1"/>"#,
                "its `<path>` has `fill-rule=\"evenodd !important\"`, not a value a template reads",
            ),
            // Round 2: a quoted `/*`; an escaped name.
            (
                r#"<path fill-rule="evenodd" style="--x:'/*'; fill-rule:nonzero; --y:'*/'" d="M0 0h1"/>"#,
                "its `<path>` has a `style`",
            ),
            (
                r#"<path fill-rule="evenodd" style="fill-\72 ule:nonzero" d="M0 0h1"/>"#,
                "its `<path>` has a `style`",
            ),
            // Round 3: parentheses, brackets, `all`.
            (
                r#"<path fill-rule="nonzero" style="--x:(;fill-rule:evenodd;)" d="M0 0h1"/>"#,
                "its `<path>` has a `style`",
            ),
            (
                r#"<path fill-rule="nonzero" style="--x:[;fill-rule:evenodd;]" d="M0 0h1"/>"#,
                "its `<path>` has a `style`",
            ),
            (
                r#"<path fill-rule="evenodd" style="all:initial" d="M0 0h4v4H0Z M1 1h2v2H1Z"/>"#,
                "its `<path>` has a `style`",
            ),
            (
                r#"<g style="all:initial"><path d="M0 0h4v4H0Z"/></g>"#,
                "its `<g>` has a `style`",
            ),
            (
                r#"<path style="fill-rule:evenodd; all:initial" d="M0 0h1"/>"#,
                "its `<path>` has a `style`",
            ),
            (
                r#"<g style="all:inherit"><path d="M0 0h1"/></g>"#,
                "its `<g>` has a `style`",
            ),
            // Round 4.
            (
                r#"<path style="display:(;fill-rule:evenodd;display:)" d="M0 0h1"/>"#,
                "its `<path>` has a `style`",
            ),
            (
                "<path fill-rule=\"evenodd\u{a0}\" d=\"M0 0h1\"/>",
                "its `<path>` has `fill-rule=\"evenodd\u{a0}\"`, not a value a template reads",
            ),
            (
                "<path fill=\"\u{a0}none\" d=\"M0 0h1\"/>",
                "its `<path>` has `fill=\"\u{a0}none\"`, not a value a template reads",
            ),
            (
                "<path style=\"fill\u{a0}-rule:evenodd\" d=\"M0 0h1\"/>",
                "its `<path>` has a `style`",
            ),
            (
                r#"<path style="fill:none;fill:rgb()" d="M0 0h1"/>"#,
                "its `<path>` has a `style`",
            ),
            (
                r#"<path fill="rgb(0,0,0,0)" d="M0 0h1"/>"#,
                "its `<path>` has `fill=\"rgb(0,0,0,0)\"`, not a value a template reads",
            ),
            (
                r#"<path fill="hsl(0,0%,0%,0)" d="M0 0h1"/>"#,
                "its `<path>` has `fill=\"hsl(0,0%,0%,0)\"`, not a value a template reads",
            ),
            (
                r#"<g fill="none"><g fill="inherit"><path d="M0 0h1"/></g></g>"#,
                "its `<g>` has `fill=\"inherit\"`, not a value a template reads",
            ),
            // The exact lists themselves: `black` (no template writes it),
            // a named colour, `inherit` on a rule, a four-digit hex, an
            // Illustrator export's `<style>` block.
            (
                r#"<path fill="black" d="M0 0h1"/>"#,
                "its `<path>` has `fill=\"black\"`, not a value a template reads",
            ),
            (
                r#"<path fill-rule="inherit" d="M0 0h1"/>"#,
                "its `<path>` has `fill-rule=\"inherit\"`, not a value a template reads",
            ),
            (
                r##"<path fill="#000f" d="M0 0h1"/>"##,
                "its `<path>` has `fill=\"#000f\"`, not a value a template reads",
            ),
            (
                r#"<defs><style>.cls-1{fill:#000}</style></defs><path class="cls-1" d="M0 0h4v4H0Z"/>"#,
                "it has a `<style>` block",
            ),
            // Round 5: namespaces, CDATA (a DOCTYPE below: it precedes the root).
            (
                r#"<s:style xmlns:s="http://www.w3.org/2000/svg">path{fill-rule:nonzero}</s:style><path fill-rule="evenodd" d="M0 0h1"/>"#,
                "its `<s:style>` has a namespace prefix",
            ),
            (
                r#"<g xmlns="http://www.w3.org/1999/xhtml"><path d="M0 0h1"/></g>"#,
                "its `<g>` declares `xmlns=\"http://www.w3.org/1999/xhtml\"`",
            ),
            (
                r#"<g xmlns="http://www.w3.org/2000/svg"><path d="M0 0h1"/></g>"#,
                "its `<g>` declares `xmlns=\"http://www.w3.org/2000/svg\"`",
            ),
            (
                r##"<path xlink:href="#a" d="M0 0h1"/>"##,
                "its `<path>` has `xlink:href`, a namespaced attribute",
            ),
            (
                r#"<![CDATA[x]]><path d="M0 0h1"/>"#,
                "it has a CDATA section",
            ),
        ];
        let doctype = read(r#"<!DOCTYPE svg [<!ATTLIST path fill-rule CDATA "evenodd">]><svg viewBox="0 0 24 24"><path d="M0 0h1"/></svg>"#).unwrap();
        assert!(
            doctype
                .not_template
                .as_deref()
                .unwrap_or("")
                .starts_with("it has a DOCTYPE"),
            "{:?}",
            doctype.not_template
        );
        let plain =
            read(r#"<!doctype svg><svg viewBox="0 0 24 24"><path d="M0 0h1"/></svg>"#).unwrap();
        assert!(
            plain
                .not_template
                .as_deref()
                .unwrap_or("")
                .starts_with("it has a DOCTYPE"),
            "{:?}",
            plain.not_template
        );
        let xmlns = read(
            r#"<svg xmlns:x="http://example.com/x" viewBox="0 0 24 24"><path d="M0 0h1"/></svg>"#,
        )
        .unwrap();
        assert_eq!(
            xmlns.not_template.as_deref(),
            Some("its `<svg>` declares `xmlns:x=\"http://example.com/x\"`")
        );
        let root = read(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M0 0h1"/></svg>"#).unwrap();
        assert_eq!(root.not_template, None, "the root's default SVG namespace");
        for (body, reason) in cases {
            let svg = format!(r#"<svg viewBox="0 0 24 24">{body}</svg>"#);
            let read = read(&svg).unwrap();
            let why = read.not_template.clone().unwrap_or_default();
            assert!(why.starts_with(reason), "{svg}: {why}");
            assert!(
                read.fix
                    .contains("drop the `style` attribute or the `<style>` block")
                    || read.fix.contains("exactly")
                    || read.fix.contains("a template is plain SVG"),
                "{svg}: {}",
                read.fix
            );
        }
    }

    /// Refusing `style` and inexact values costs no SVG the repo ships:
    /// every one under a package's or an app's `assets/` reads with no
    /// `style`, `<style>` or value reason (whatever else it is: an avatar
    /// picture's rectangle is no template anyway).
    #[test]
    fn every_repo_svg_reads_past_the_style_rule() {
        fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for e in entries.flatten() {
                let (path, name) = (e.path(), e.file_name().to_string_lossy().into_owned());
                if path.is_dir()
                    && !matches!(name.as_str(), "node_modules" | "target" | "dist")
                    && !name.starts_with('.')
                {
                    walk(&path, out);
                } else if name.ends_with(".svg")
                    && path.components().any(|c| c.as_os_str() == "assets")
                {
                    out.push(path);
                }
            }
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut svgs = Vec::new();
        walk(&root.join("packages"), &mut svgs);
        walk(&root.join("apps"), &mut svgs);
        assert!(svgs.len() > 15, "{svgs:?}");
        let refused: Vec<String> = svgs
            .iter()
            .filter_map(|p| {
                let text = std::fs::read_to_string(p).ok()?;
                let why = read(&text).ok()?.not_template?;
                (why.contains("`style`")
                    || why.contains("`<style>`")
                    || why.contains("not a value a template reads")
                    || why.contains("`color="))
                .then(|| format!("{}: {why}", p.display()))
            })
            .collect();
        assert!(refused.is_empty(), "{refused:#?}");
    }

    /// The in-repo packages' icon SVGs (`packages/*/assets/`) are each a
    /// template the lint accepts, their path the file's `d`, and none is
    /// even-odd: the ones that were (stage 14 C: ComposeFrame's three, the
    /// Attie mark, the badges' paths) fill the same nonzero, so they are
    /// written so, and no app using the packages takes `iconFillRules`.
    #[test]
    fn every_package_icon_svg_is_a_nonzero_template() {
        let packages = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages");
        let mut seen = 0;
        for package in std::fs::read_dir(&packages).unwrap() {
            let Ok(files) = std::fs::read_dir(package.unwrap().path().join("assets")) else {
                continue;
            };
            for file in files {
                let path = file.unwrap().path();
                if path.extension().is_none_or(|e| e != "svg") {
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap();
                let svg = read(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                assert_eq!(svg.not_template, None, "{}", path.display());
                assert!(!svg.even_odd, "{}", path.display());
                // Every path's `d`, joined by `;` as the plan carries them.
                let d: Vec<&str> = text
                    .split(" d=\"")
                    .skip(1)
                    .filter_map(|t| t.split('"').next())
                    .collect();
                assert_eq!(svg.path, d.join(";"), "{}", path.display());
                seen += 1;
            }
        }
        assert!(seen >= 23, "{seen} package SVGs");
    }
}
