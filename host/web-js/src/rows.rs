//! Dynamic style rows (rt.js `S`, `Sm`), their inline static rows, and bound `data-*` words.

use super::Em;
use crate::style;
use exact_kernel::{NodeType, PropId, StyleId};
use exact_plan::{BindingKind, BindingsRow, NodesId};
use exact_web::host::template::Parts;
use std::fmt::Write as _;

#[path = "text_style.rs"]
mod text_style;
impl Em<'_> {
    /// Clock ticks format cached instants without evaluating app code.
    pub(super) fn live_time(
        &mut self,
        i: u32,
        element: &str,
        e: &str,
        scope: &crate::code::Scope,
    ) -> Result<(), String> {
        let plan = self.plan;
        let row = &plan.node(NodesId(i));
        if element == "time"
            || (element == "progress"
                && row
                    .bindings
                    .iter()
                    .any(|b| plan.binding(b).id == PropId::Start as u16))
        {
            let mut values = Vec::new();
            for prop in [
                PropId::Datetime,
                PropId::TimeFormat,
                PropId::Start,
                PropId::End,
            ] {
                let value = row
                    .bindings
                    .iter()
                    .map(|b| plan.binding(b))
                    .find(|b| b.kind == BindingKind::Prop && b.id == prop as u16);
                values.push(match value {
                    Some(b) => format!("({})()", self.bound(i, e, &b, scope)?),
                    None => "null".into(),
                });
            }
            let live = self.uses.rt("liveTime");
            let inline = row
                .parent
                .is_some_and(|p| plan.node(p).node_type == NodeType::Text as u8);
            let reserve = !inline
                && !row.bindings.iter().map(|b| plan.binding(b)).any(|b| {
                    b.kind == BindingKind::Style
                        && (b.id == StyleId::Width as u16 || b.id == StyleId::MaxWidth as u16)
                });
            let _ = write!(
                self.out,
                "{live}({e},()=>[{}],{reserve});",
                values.join(",")
            );
        }
        Ok(())
    }

    /// Whether image `i` needs symbols.js for a symbol, tint, bound source or
    /// `app:/` file (`data-app-src`, LLP 1069.002 D7), and whether its source is bound.
    pub(super) fn image_piece(&self, i: u32, parts: &Parts) -> Option<bool> {
        let plan = self.plan;
        let row = plan.node(NodesId(i));
        let bound = row.bindings.iter().map(|b| plan.binding(b)).any(|b| {
            b.kind == BindingKind::Prop
                && b.id == PropId::ImageSource as u16
                && style::literal(plan, plan.code(b.expr)).is_none()
        });
        let src = parts.props.get("src");
        let app = src.is_some_and(|s| s.starts_with("app:/"));
        let symbol = parts.props.contains_key("data-symbol-path");
        let tint = parts.css.contains("--exact-tint-offset:");
        (bound || app || symbol || tint).then_some(bound)
    }

    /// @ref LLP 1055.002 — a synced animation: the web host's clocks
    /// (navigation.js `animationClocks`, through rt.js), made once, set a
    /// joined animation's start after each commit; under the agent its
    /// register does (agent.js).
    pub(super) fn clocks(&mut self) {
        let (clocks, after, clock) = (
            self.uses.rt("animationClocks"),
            self.uses.rt("After"),
            self.uses.rt("clock"),
        );
        let _ = write!(
            self.out,
            "if(!globalThis.__exactClocks&&typeof requestAnimationFrame==\"function\"&&!globalThis.__exactRender){{const c=globalThis.__exactClocks={clocks}(document);{after}.push(()=>{clock}.agent||c.sync());}}"
        );
    }

    /// [`Self::clocks`] when a static declaration puts the node on a clock.
    pub(super) fn clocks_in(&mut self, declarations: &str) {
        if declarations.contains("--exact-animation-clock:") {
            self.clocks();
        }
    }

    pub(super) fn f(
        &mut self,
        code: exact_plan::Code,
        scope: &crate::code::Scope,
    ) -> Result<String, String> {
        crate::code::function(self.plan, self.plan.code(code), scope, 0, &mut self.uses)
    }

    /// A bound paint fact for the CSS sibling-order rule. The expression is
    /// pure; the same effect scope as its style owns this attribute.
    pub(super) fn paint_binding(&mut self, kind: NodeType, b: &BindingsRow, e: &str, f: &str) {
        if !self.paint || kind.is_svg_element() || kind.is_metadata() {
            return;
        }
        if let Some((name, value)) = crate::paint::binding(self.plan, kind, b, |parser| {
            parser.emit(self.plan, &mut self.uses)
        }) {
            let f = if b.kind == BindingKind::Style && b.id == StyleId::BackdropFilter as u16 {
                format!("()=>{}(({f})(),false)", self.uses.rt("backdropValue"))
            } else {
                f.to_string()
            };
            let p = self.uses.rt("P");
            let _ = write!(
                self.out,
                "{p}({e},\"{name}\",()=>{{const v=({f})();return {value}}});"
            );
        }
    }

    /// A development build's binding `k` of node `i`, counted (LLP 1079 D1,
    /// perf.js): declared once, as `p{i}_{k}`, which every write it feeds
    /// reads, so a mapped write does not make a counter per evaluation.
    pub(super) fn counted(&mut self, i: u32, k: usize, f: String) -> String {
        if !self.site_attrs {
            return f;
        }
        let pf = self.uses.rt("pf");
        let _ = write!(self.out, "const p{i}_{k}={pf}({i},{f});");
        format!("p{i}_{k}")
    }

    pub(super) fn style_row(
        &mut self,
        i: u32,
        b: &BindingsRow,
        parts: &Parts,
        e: &str,
        f: &str,
        scope: &crate::code::Scope,
    ) -> Result<(), String> {
        let plan = self.plan;
        let row = &plan.node(NodesId(i));
        let shift = parts.css.contains("--exact-translate-composes:1");
        let binding = |id: StyleId| {
            row.bindings
                .iter()
                .map(|b| plan.binding(b))
                .find(|b| b.kind == BindingKind::Style && b.id == id as u16)
        };
        let press = parts.css.contains("--exact-press:")
            || binding(StyleId::PressScale)
                .is_some_and(|b| style::literal(plan, plan.code(b.expr)).is_none());
        // A clock (LLP 1055.002) plays on the page's timeline: only a drag
        // timeline's consumer is paused for the drag to seek.
        let clock = |s: &str| s.trim_start().starts_with("-exact-clock(");
        let timeline = binding(StyleId::AnimationTimeline)
            .is_some_and(|t| style::can_be(plan, plan.code(t.expr), &|s| !clock(s)));
        let id = StyleId::from_bit(b.id as u32).ok_or("unknown style row")?;
        let kind = NodeType::from_wire(row.node_type).ok_or("unknown node type")?;
        let checked_animation = self.svg_animation(i, id, e, f, scope)?;
        let f = checked_animation.as_deref().unwrap_or(f);
        // @ref LLP 1115.002 D6 — a metric the binding may name is its value
        // in the web's column before the row's own map sees it.
        let metric = style::metric_map(plan, plan.code(b.expr))
            .map(|m| format!("()=>{}(({f})())", self.uses.share('m', m)));
        let f = metric.as_deref().unwrap_or(f);
        // @ref LLP 1121 D7 — a length or number token a bound value names is
        // its CSS, or its property's `var()` where the page declares one.
        let attribute = exact_web::host::template::attribute_rows(kind)
            .iter()
            .any(|(r, _)| *r == id);
        // A node with children reads no varying type or family token: they
        // would inherit it (the runner refuses it as an invalid value).
        // (Asked only where one varies: a map's name stays as it was.)
        let container = self.answers.varying && !self.sites.of_node(i).is_empty();
        let measured = if crate::measures::row(id) {
            self.answers.map(id, attribute, container)
        } else {
            None
        };
        let measure = measured.map(|m| {
            let bases = !crate::base_scale::named(plan).is_empty();
            let name = crate::measures::map_name(id, attribute, container, bases);
            let map = self.uses.constant(&name, || m.to_string());
            format!("()=>{map}(({f})())")
        });
        let f = measure.as_deref().unwrap_or(f);
        let length = style::length_value(id, f, &mut self.uses);
        let f = length.as_deref().unwrap_or(f);
        // A list on a corner is the elliptical radius no row holds, which
        // the browser would draw: refused whole, as the kernel refuses it.
        let corner = (exact_kernel::style::sides::FAMILIES[8].contains(&id)
            && style::literal(plan, plan.code(b.expr)).is_none())
        .then(|| {
            self.uses.rt("say");
            format!(
                "()=>{}(({f})())",
                self.uses.share('m', style::corner_list(id))
            )
        });
        let f = corner.as_deref().unwrap_or(f);
        if let Some((_, name)) = exact_web::host::template::attribute_rows(kind)
            .iter()
            .find(|(attribute, _)| *attribute == id)
        {
            let p = self.uses.rt("svgGeometry");
            let css = self
                .uses
                .rt(if self.is_motion_node(i) { "Sm" } else { "S" });
            let _ = write!(self.out, "{p}({e},\"{name}\",{f},{css});");
            return Ok(());
        }
        if matches!(
            id,
            StyleId::GridTemplateColumns
                | StyleId::GridTemplateRows
                | StyleId::GridColumn
                | StyleId::GridRow
                | StyleId::GridAutoFlow
                | StyleId::JustifyItems
        ) {
            self.uses.rt("gridValue");
        }
        if id == StyleId::AnimationTimeline && style::can_be(plan, plan.code(b.expr), &clock) {
            self.clocks();
        }
        // A bound pin: modes.js gives the boundary its tuple (LLP 1121 D7).
        if id == StyleId::Mode {
            let p = self.uses.rt("P");
            let _ = write!(self.out, "{p}({e},\"data-x-pin\",{f});");
            return Ok(());
        }
        // Mark bound rows while they are declared, a clear included, as the
        // kernel's written bit marks a literal one (data-exact-said): grouped
        // separators yield to authored edges and images (LLP 1084 D7), and a
        // native button's floor yields to authored size and padding.
        let button = kind == NodeType::Control && parts.props.contains_key("data-button-style");
        let mark = match id {
            StyleId::BorderWidthTop if b.written => Some("data-exact-top-width"),
            StyleId::BorderStyleTop if b.written => Some("data-exact-top-style"),
            StyleId::BorderWidthBottom if b.written => Some("data-exact-bottom-width"),
            StyleId::BorderStyleBottom if b.written => Some("data-exact-bottom-style"),
            StyleId::BackgroundImage if b.written => Some("data-exact-background-image"),
            StyleId::Display
                if parts.props.contains_key("data-groupedrowseparator")
                    || parts.props.contains_key("data-native") =>
            {
                Some("data-exact-hidden")
            }
            StyleId::Width if b.written && button => Some("data-exact-width"),
            StyleId::Height if b.written && button => Some("data-exact-height"),
            StyleId::PaddingTop if b.written && button => Some("data-exact-padding-top"),
            StyleId::PaddingRight if b.written && button => Some("data-exact-padding-right"),
            StyleId::PaddingBottom if b.written && button => Some("data-exact-padding-bottom"),
            StyleId::PaddingLeft if b.written && button => Some("data-exact-padding-left"),
            _ => None,
        };
        if let Some(name) = mark {
            let p = self.uses.rt("P");
            let when = if id == StyleId::Display {
                "v===\"none\"?\"\":null"
            } else {
                "v==null?null:\"\""
            };
            let _ = write!(
                self.out,
                "{p}({e},\"{name}\",()=>{{const v=({f})();return {when}}});"
            );
        }
        let refuse = |why: &str| Err(format!("node {i}: {why} is not in the JS target"));
        // @ref LLP 1077 D13 — a bound colour may name a system colour, which
        // the literal path resolved in the kernel. And a CSS-text row that
        // may name one inside it. The plan's colour map: one module constant
        // every such site shares.
        use exact_kernel::StyleCodec as C;
        let cm = matches!(
            id.codec(),
            C::ColorValue
                | C::KeywordColor
                | C::Paint
                | C::BackgroundImage
                | C::MaskImage
                | C::BoxShadow
                | C::TextShadow
                | C::Filter
        )
        .then(|| self.uses.constant("$cm", || style::color_map(plan)));
        // (name, unit, map): a map is JavaScript of the value (`null` writes none).
        let one = |name: &str, map: Option<String>| vec![(name.to_string(), String::new(), map)];
        let mut writes: Vec<(String, String, Option<String>)> = match id {
            // Safari selects by WebKit's prefixed property alone; `auto` and a
            // clear write nothing, as css.rs writes them (css/selection.rs,
            // css/clears.rs).
            StyleId::UserSelect => {
                let unsaid = || Some("v=>/^\\s*(auto|initial|unset)\\s*$/i.test(v)?null:v".to_string());
                vec![
                    ("-webkit-user-select".into(), String::new(), unsaid()),
                    ("user-select".into(), String::new(), unsaid()),
                ]
            }
            StyleId::PressTranslate => {
                let press = self.uses.rt("pressFeedback");
                let _ = write!(self.out, "{press}();");
                style::press_translation_writes()
            }
            // @ref LLP 1077 D8 — rotation keeps its axis beside the angle;
            // each translation's parts share the author's whole text.
            StyleId::RotateAxis => one("--exact-rotate-axis", Some("v=>{const w=String(v??'').trim().split(/\\s+/);if(w.length===2)return w.find(x=>/^[xyz]$/i.test(x))||'z';return w.length===4?w.slice(0,3).join(' '):'z'}".into())),
            StyleId::TranslateZ | StyleId::TranslatePercent | StyleId::PressTranslatePercent => {
                let pair = if id == StyleId::PressTranslatePercent { StyleId::PressTranslate } else { StyleId::Translate };
                if binding(pair).is_some_and(|o| plan.code(o.expr) == plan.code(b.expr)) { return Ok(()); }
                return refuse("a dynamic 3D part without its `rotate` or `translate`");
            }
            // @ref LLP 1077 D7 — the shorthand binds both rows with the
            // author's whole text: one declaration of the shorthand.
            StyleId::TextStrokeWidth | StyleId::TextStrokeColor
                if binding(if id == StyleId::TextStrokeWidth { StyleId::TextStrokeColor } else { StyleId::TextStrokeWidth })
                    .is_some_and(|o| plan.code(o.expr) == plan.code(b.expr)) =>
            {
                if id == StyleId::TextStrokeColor {
                    return Ok(());
                }
                one("-webkit-text-stroke", Some(self.uses.constant("$cm", || style::color_map(plan))))
            }
            // @ref LLP 1077 D8 — 0 is `none`, as css.rs writes it.
            StyleId::Perspective => one(
                "perspective",
                Some("v=>v==null?v:/^\\s*[+-]?(0+\\.?0*|\\.0+)(px)?\\s*$/i.test(v)?\"none\":typeof v===\"number\"?`${v}px`:v".into()),
            ),
            StyleId::BackdropFilter => {
                let check = self.uses.rt("backdropValue");
                one("backdrop-filter", Some(format!("v=>{check}(v)")))
            }
            // @ref LLP 1093 §1 — the row's 0 is CSS `auto`, as css.rs writes
            // it. `setProperty("column-count", "0")` does not stick, so the
            // unit sample would journal the refusal and leave a class rule.
            StyleId::ColumnCount => one(
                "column-count",
                Some("v=>v==null?v:typeof v===\"number\"?(v===0?\"auto\":v):v".into()),
            ),
            // A stack index: css.rs's declaration for each, by index.
            StyleId::FontFamily => {
                let table = serde_json::to_string(&style::font_family_table(plan)).unwrap();
                let runtime = self.uses.rt("fontFamily");
                one("font-family", Some(format!("v=>typeof v===\"string\"?{runtime}(v):{table}[v]??null")))
            }
            // css.rs's legacy clamp, on a non-scrolling block only.
            StyleId::Display if parts.props.contains_key("data-button-style") =>
                one("display", Some("v=>v===\"none\"?\"none\":\"grid\"".into())),
            StyleId::LineClamp if parts.props.contains_key("data-button-style") => vec![
                ("--exact-button-clamp".into(), String::new(), Some("v=>v>0?v:null".into())),
                ("--exact-button-title-display".into(), String::new(), Some("v=>v>0?\"-webkit-box\":null".into())),
                ("--exact-button-title-overflow".into(), String::new(), Some("v=>v>0?\"hidden\":null".into())),
            ],
            StyleId::LineClamp => {
                let display = parts.css.split(';').find_map(|d| d.strip_prefix("display:"));
                if display.is_some_and(|d| d != "block")
                    || parts.css.contains("overflow-x:scroll")
                    || parts.css.contains("overflow-y:scroll")
                    || parts.css.contains("overflow-x:auto")
                    || parts.css.contains("overflow-y:auto")
                {
                    self.warnings.push(format!(
                        "node {i}: style row line_clamp skipped: legacy line-clamp requires a non-scrolling block"
                    ));
                    return Ok(());
                }
                let when = |v: &str| Some(format!("v=>v>0?{v}:null"));
                vec![
                    ("-webkit-line-clamp".into(), String::new(), when("v")),
                    ("display".into(), String::new(), when("\"-webkit-box\"")),
                    ("-webkit-box-orient".into(), String::new(), when("\"vertical\"")),
                    ("overflow".into(), String::new(), when("\"hidden\"")),
                    ("--exact-button-clamp".into(), String::new(), Some("v=>v==null?null:v>0?v:0".into())),
                    ("--exact-button-title-display".into(), String::new(), Some("v=>v==null?null:v>0?\"-webkit-box\":\"block\"".into())),
                    ("--exact-button-title-overflow".into(), String::new(), Some("v=>v==null?null:v>0?\"hidden\":\"clip\"".into())),
                ]
            }
            // @ref LLP 1077 D14 — host-owned, as `-exact-press-scale`: the custom
            // property input-glue.js plays at the press (css.rs).
            StyleId::PressHaptic => {
                let press = self.uses.rt("pressFeedback");
                let _ = write!(self.out, "{press}();");
                one("--exact-press-haptic", Some("v=>v==null||v===\"none\"?null:v".into()))
            }
            // @ref LLP 1061 D7 — the pressed paint: the custom properties
            // input-glue.js reads from the element's own style (css.rs).
            StyleId::PressOpacity
            | StyleId::PressOpacityTo
            | StyleId::PressBackgroundColor
            | StyleId::PressColor => {
                self.press_paint = true;
                let press = self.uses.rt("pressFeedback");
                let _ = write!(self.out, "{press}();");
                let name = match id {
                    StyleId::PressOpacity => "--exact-press-opacity",
                    StyleId::PressOpacityTo => "--exact-press-opacity-to",
                    StyleId::PressBackgroundColor => "--exact-press-background-color",
                    _ => "--exact-press-color",
                };
                let none = "v==null||/^\\s*unset\\s*$/i.test(v)?null:";
                one(name, Some(match &cm {
                    Some(cm) => format!("v=>{none}{cm}(v)"),
                    None => format!("v=>{none}v"),
                }))
            }
            // @ref LLP 1116.001 D1.4, LLP 1077 D12 — what navigation.js `fitText` and
            // `playSymbolEffects` read, inline.
            StyleId::Fit | StyleId::SymbolEffect => {
                let (piece, name, map) = match id {
                    StyleId::Fit => ("fitTexts", "--exact-fit", "v=>v===\"shrink\"?v:null"),
                    _ => ("symbolEffects", "--exact-symbol-effect", "v=>/^(bounce|pulse|scale)$/.test(v)?v:null"),
                };
                let piece = self.uses.rt(piece);
                let _ = write!(self.out, "{piece}();");
                one(name, Some(map.into()))
            }
            StyleId::PressScale => {
                if binding(StyleId::Scale).is_some() || binding(StyleId::Animation).is_some() {
                    return refuse("a dynamic `-exact-press-scale` beside `scale` or `animation`");
                }
                let press = self.uses.rt("pressFeedback");
                let _ = write!(self.out, "{press}();");
                vec![
                    ("--exact-press".into(), String::new(), Some("v=>v==null||v===1||/^\\s*unset\\s*$/i.test(v)?null:v".into())),
                    (
                        "scale".into(),
                        String::new(),
                        Some("v=>v==null||v===1||/^\\s*unset\\s*$/i.test(v)?null:\"calc(var(--exact-scale,1) * var(--exact-press-factor,1))\"".into()),
                    ),
                ]
            }
            StyleId::FontSize if parts.tag != "img" => {
                let styles = self.answers.styles();
                text_style::font_size(plan, row, parts, styles, &mut |map| self.uses.share('m', map))
            }
            StyleId::FontSize | StyleId::FontWeight if parts.tag == "img" => {
                let size = id == StyleId::FontSize;
                let map = size.then(|| self.uses.share('m', style::TEXT_STYLE_MAP.to_string()));
                let mut writes = vec![(if size { "font-size" } else { "font-weight" }.into(), if size { "px" } else { "" }.into(), map)];
                writes.push((if size { "--exact-symbol-size-authored" } else { "--exact-symbol-weight-authored" }.into(), String::new(), Some("v=>v==null?null:1".into())));
                writes
            }
            StyleId::FontVariantNumeric => one("font-variant-numeric", None),
            StyleId::FontVariantLigatures => one("font-variant-ligatures", None),
            // SVG's transform grammar, restated as CSS's (kernel TransformList).
            StyleId::Transform => {
                let t = self.uses.rt("svgTransform");
                one("transform", Some(format!("v=>{t}(v)")))
            }
            StyleId::MarkerStart | StyleId::MarkerMid | StyleId::MarkerEnd => {
                let (name, _) = style::style_marker(id);
                one(&name, Some("v=>v==null||/^\\s*none\\s*$/i.test(v)?null:v".into()))
            }
            // Animate the authored transform bases under the press factors
            StyleId::Translate if shift => ["x", "y", "z"].iter().enumerate().map(|(i, axis)| (
                format!("--exact-translate-{axis}"), String::new(), Some(format!(
                    r#"v=>{{const text=String(v??'').trim();if(!CSS.supports('translate',text))return '0px';const p=text.match(/calc\([^)]*\)|[^\s]+/g)??[];return !p.length||/^(none|unset|initial)$/i.test(p[0])?'0px':p[{i}]??'0px'}}"#
                ))
            )).chain(one("translate", None)).chain(one("--exact-base-translated", Some("v=>CSS.supports('translate',String(v))&&!/^(none|unset|initial|revert|revert-layer)$/i.test(String(v).trim())?'1':null".into()))).collect(),
            StyleId::Scale if press => one("--exact-scale", None),
            StyleId::Animation if press || shift => {
                let names: Vec<String> = self.press_keyframes.clone();
                let names = serde_json::to_string(&names).unwrap();
                let mut w = vec![(
                    "animation".to_string(),
                    String::new(),
                    Some(format!("v=>v==null||/^\\s*none\\s*$/i.test(v)?null:v.split(/,(?![^(]*\\))/).map(p=>p.replace(/[\\w-]+/g,n=>{names}.includes(n)?n+\"-exact-press\":n)).join(\",\")")),
                )];
                if timeline {
                    w.push(("animation-play-state".into(), String::new(), Some("v=>v==null||/^\\s*none\\s*$/i.test(v)?null:\"paused\"".into())));
                }
                w
            }
            StyleId::Transition => {
                let spring = self.uses.constant("$transitionSpring", style::transition_spring);
                let entries = self.uses.constant("$transitionEntries", || style::transition_entries(&spring));
                let mut writes = one("transition", Some(self.uses.share('m', style::transition_map(&entries, press, shift))));
                if exact_web::css::PRESS_ROWS.iter().any(|(id, _)| binding(*id).is_some()) {
                    let parser = crate::paint::Parser::Transition.emit(plan, &mut self.uses);
                    let map = crate::paint::press_timing_map(&parser, &spring, b.written);
                    writes.extend(one("--exact-press-timing", Some(self.uses.share('m', map))));
                }
                writes
            }
            StyleId::LayoutTransition => {
                let parser = crate::paint::Parser::Transition.emit(plan, &mut self.uses);
                let (say, view) = (self.uses.rt("say"), self.uses.rt("viewId"));
                one("--exact-layout-transition", Some(crate::presence::layout_transition_map(&parser, &format!("{view}({e})"), &say)))
            }
            StyleId::ExitAnimation => {
                let values = crate::presence::exit_animations(plan, plan.code(b.expr), press || shift);
                let (say, view) = (self.uses.rt("say"), self.uses.rt("viewId"));
                one("--exact-exit-animation", Some(crate::presence::declaration_map(&values, &format!("{view}({e})"), &say, "exit-animation")))
            }
            // @ref LLP 1013.001 D1 — a route's entrance and exit, read by
            // shared.js from the route's own style as the change happens.
            StyleId::ViewTransitionNew | StyleId::ViewTransitionOld => {
                let values = crate::presence::route_parts(plan, plan.code(b.expr), id);
                let (say, view) = (self.uses.rt("say"), self.uses.rt("viewId"));
                let name = crate::presence::route_part_property(id);
                one(name, Some(crate::presence::declaration_map(&values, &format!("{view}({e})"), &say, &name[2..])))
            }
            // A side is the author's when they wrote it or the other of its
            // pair (css.rs `unsaid_side`): only a side the sheet alone sets mirrors.
            _ => style::style_writes(b.id, timeline, b.written || style::other_side(id).and_then(binding).is_some_and(|o| o.written))
                .map_err(|x| format!("node {i}: {x}"))?
                .into_iter()
                .map(|w| {
                    // Composed with the row's own map (`accent-color` has one).
                    let map = match (w.map, &cm) {
                        (Some(m), Some(cm)) => Some(format!("v=>({m})({cm}(v))")),
                        (Some(m), None) => Some(m.to_string()),
                        (None, cm) => cm.clone(),
                    };
                    // A computed image the native hosts refuse is dropped
                    // here too, and journaled as they journal it, so the web
                    // never paints what a Mac drops (studio diary R15).
                    let map = if matches!(id, StyleId::BackgroundImage | StyleId::MaskImage) {
                        let names = exact_kernel::gradient::REFUSED
                            .iter()
                            .map(|(p, _)| p.trim_end_matches('('))
                            .collect::<Vec<_>>()
                            .join("|");
                        let inner = map.unwrap_or_else(|| "v=>v".into());
                        Some(format!(
                            "v=>{{if(v!=null&&/(^|[^a-z0-9-])({names})\\(/i.test(v)){{const x=globalThis.exact;x?.journal?.push(`t=${{x.now?.()??0}} invalid {} value ${{JSON.stringify(String(v))}}; unset`);return \"unset\"}}return({inner})(v)}}",
                            id.name().replace('_', "-")
                        ))
                    } else {
                        map
                    };
                    (w.name, w.unit, map)
                })
                .collect(),
        };
        // A progress's or canvas's alignment, and its parent's (sized.rs).
        let sized = crate::sized::writes(id, parts);
        if !sized.is_empty() {
            self.uses.rt("normal");
        }
        let play_state = crate::timelines::play_state(self.drag_timelines);
        if id == StyleId::AnimationTimeline && !self.drag_timelines {
            crate::timelines::css_writes(&mut writes);
        }
        // An animation shorthand resets the play state, the timeline and the
        // range, so read them live again after it.
        if id == StyleId::Animation && timeline {
            let getter = self.bound(i, e, &binding(StyleId::AnimationTimeline).unwrap(), scope)?;
            for (name, _, map) in &mut writes {
                if name == "animation-play-state" {
                    let inner = map.take().unwrap();
                    *map = Some(format!(
                        "v=>({inner})(v)==null?null:({play_state})(({getter})())"
                    ));
                }
            }
            let range = (binding(StyleId::AnimationRange))
                .map(|r| self.bound(i, e, &r, scope))
                .transpose()?;
            crate::timelines::after_shorthand(&mut writes, self.drag_timelines, &getter, range);
        }
        writes.extend(sized);
        if matches!(id, StyleId::FontWeight | StyleId::LineHeight) {
            let normal = self.uses.rt("normal");
            // The marker depends on both rows: either can expose an inherited value.
            let start = if parts.props.contains_key("data-fontshorthand") {
                binding(StyleId::FontSize)
                    .filter(|b| style::literal(plan, plan.code(b.expr)).is_none())
                    .map(|b| {
                        let size = self.bound(i, e, &b, scope)?;
                        let styles = self.answers.styles();
                        let map = self.uses.share(
                            'm',
                            text_style::style_map(styles, &style::TEXT_STYLE_ROWS[0]),
                        );
                        Ok::<_, String>(format!("({map})(({size})())"))
                    })
                    .transpose()?
            } else {
                None
            };
            text_style::longhand(plan, row, parts, id, &normal, start.as_deref(), &mut writes);
        }
        // @ref LLP 1084 D7 — a grouped row's separator stands below its
        // bottom padding (the page's sheet, as `element::host_css_of` says).
        if id == StyleId::PaddingBottom && parts.props.contains_key("data-groupedrowseparator") {
            if let Some((_, unit, map)) = writes.first().cloned() {
                // A length; a percentage, which a background cannot reach,
                // is none (`element::host_css_of`).
                let inner = map.unwrap_or_else(|| "v=>v".into());
                let px = format!(
                    "v=>{{const t=({inner})(v);return t!=null&&/%/.test(String(t))?\"0px\":t}}"
                );
                writes.push(("--exact-padding-bottom".into(), unit, Some(px)));
            }
        }
        if id == StyleId::Appearance && b.written {
            self.said_appearance(e, f, parts.props.get("data-exact-said"));
        }
        // Right to left, the sides of a pair different hands say (sides.rs).
        let other = style::other_side(id).and_then(binding);
        self.side_writes(
            i,
            e,
            scope,
            b,
            other.as_ref(),
            &parts.css,
            timeline,
            &mut writes,
        )?;
        // A reference (`url(#…)`) names an element by its authored id, which
        // the kernel scopes to the instance (LLP 1055.000 D3): resolved at
        // run time from the node (`Sr`).
        let refs = style::can_be(plan, plan.code(b.expr), &|v| v.contains("url("));
        let s = self.uses.rt(if refs {
            "Sr"
        } else if self.is_motion_node(i) {
            "Sm"
        } else {
            "S"
        });
        // @ref LLP 1121 D7 — an inherited colour row says whether it read
        // a token (modes.js `Sx`), which a boundary below writes again (a
        // stroke's shorthand by its colour). A boundary declares the tokens,
        // so its `--exact-accent` naming one that names the accent would name
        // itself: that token's colour stands by tuple (`data-x-accent`), as
        // css.rs leaves a self-reference out; a clear there writes nothing.
        // A blank text row clears its start; `none` leaves that start exposed.
        let clear = matches!(id, StyleId::FontWeight | StyleId::LineHeight).then(|| {
            format!("()=>{{const v=({f})();return typeof v===\"string\"&&!v.trim()?\"unset\":v}}")
        });
        let f = clear.as_deref().unwrap_or(f);
        let clear_css = style::clear_css(id, press);
        for (name, unit, map) in writes {
            let modes = self.answers.modes;
            let channel = crate::modes::CHANNELS
                .iter()
                .position(|(_, p)| {
                    *p == name || name == "-webkit-text-stroke" && *p == "-webkit-text-stroke-color"
                })
                .filter(|_| modes);
            let clear = style::cleared(&clear_css, &name)
                .filter(|_| {
                    id != StyleId::Display || !parts.props.contains_key("data-button-style")
                })
                .map(|v| format!(",{}", serde_json::to_string(v).unwrap()));
            let map =
                map.or_else(|| (name.starts_with("--") && clear.is_some()).then(|| "v=>v".into()));
            let value = match map {
                Some(m) => format!(
                    "()=>{}(({f})(),{m}{})",
                    self.uses.rt("mapped"),
                    clear.unwrap_or_default()
                ),
                None => f.to_string(),
            };
            let value = if modes && name == "--exact-accent" {
                format!(
                    "()=>{{const v=({value})();if(!{e}.hasAttribute(\"data-x-b\"))return v;{e}.removeAttribute(\"data-x-accent\");if(v==null||/^\\s*(unset|inherit)?\\s*$/i.test(v))return null;const n={}.find(p=>v.includes(`var(${{p}})`));if(n){{{e}.setAttribute(\"data-x-accent\",n);return null}}return v}}",
                    self.answers.accent()
                )
            } else {
                value
            };
            match channel {
                Some(j) => {
                    let _ = write!(self.out, "Sx({s},{e},\"{name}\",\"{unit}\",{value},{j});");
                }
                None => {
                    let _ = write!(self.out, "{s}({e},\"{name}\",\"{unit}\",{value});");
                    // The root's bound background is the page's canvas too (no modes channel carries it).
                    if i == self.sites.root && id == StyleId::BackgroundColor {
                        let (write, opaque) = (self.uses.rt("S"), self.uses.rt("canvasColor"));
                        self.out
                            .push_str(&crate::canvas::bound(&write, &opaque, &value));
                    }
                }
            }
        }
        // Probe the new region value while presence can still measure the old box.
        if id == StyleId::LayoutTransition {
            let parser = crate::paint::Parser::Transition.emit(plan, &mut self.uses);
            let lt = self.uses.rt("lt");
            let _ = write!(self.out, "{lt}({e},{parser},{f});");
        }
        Ok(())
    }

    /// A Markdown text field is the web host's own editor (LLP 1045 D5:
    /// markup-editor.js over its wasm), which replaces it once loaded; a
    /// plain one with `decorations` keeps its textarea and is painted.
    pub(super) fn editor(&mut self, i: u32, element: &str, e: &str) -> Result<(), String> {
        let plan = self.plan;
        let row = &plan.node(NodesId(i));
        let markup = row
            .bindings
            .iter()
            .map(|b| plan.binding(b))
            .find(|b| b.kind == BindingKind::Prop && b.id == PropId::Markup as u16);
        if element == "textarea" {
            if row
                .bindings
                .iter()
                .map(|b| plan.binding(b))
                .any(|b| b.kind == BindingKind::Prop && b.id == PropId::Decorations as u16)
            {
                // Paint only (LLP 1114.007): decorations.js, no editor wasm.
                let decorate = self.uses.rt("decorate");
                let _ = write!(self.out, "{decorate}({e});");
            }
            match markup.map(|b| style::literal(plan, plan.code(b.expr))) {
                Some(Some(v)) if v.as_str() == Some("markdown") => {
                    self.editor = true;
                    let mde = self.uses.rt("mde");
                    let _ = write!(self.out, "{mde}({e});");
                }
                Some(None) => {
                    return Err(format!(
                        "node {i}: a dynamic `markup` on a text field is not in the JS target"
                    ))
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// A node's `wrap-flow` (LLP 1043.000), for the text flow piece
    /// (rt.js `wf`): an absolutely positioned `both` is an exclusion.
    pub(super) fn wrap_flow(
        &mut self,
        i: u32,
        e: &str,
        scope: &crate::code::Scope,
    ) -> Result<(), String> {
        let plan = self.plan;
        let Some(b) = plan
            .node(NodesId(i))
            .bindings
            .iter()
            .map(|b| plan.binding(b))
            .find(|b| b.kind == BindingKind::Style && b.id == StyleId::WrapFlow as u16)
        else {
            return Ok(());
        };
        let value = match style::literal(plan, plan.code(b.expr)) {
            Some(v) => format!("()=>{}", super::value_js(&v)),
            None => self.bound(i, e, &b, scope)?,
        };
        self.flow = true;
        let wf = self.uses.rt("wf");
        let _ = write!(self.out, "{wf}({e},{value});");
        Ok(())
    }

    /// Whether node `i` is a `symbol` or inside one (through regions' arms).
    pub(super) fn in_symbol(&self, i: u32) -> bool {
        let plan = self.plan;
        let mut at = Some(i);
        while let Some(n) = at {
            let row = &plan.node(NodesId(n));
            if NodeType::from_wire(row.node_type) == Some(NodeType::SvgSymbol) {
                return true;
            }
            at = self.parent_of(n);
        }
        false
    }

    /// Node `i`'s parent node, through regions' arms.
    pub(super) fn parent_of(&self, i: u32) -> Option<u32> {
        let plan = self.plan;
        let row = &plan.node(NodesId(i));
        match (row.parent, row.arm) {
            (Some(p), _) => Some(p.0),
            (None, Some(a)) => {
                let mut region = plan.arms[a.0 as usize].region;
                loop {
                    let r = &plan.regions[region.0 as usize];
                    match (r.parent, r.arm) {
                        (Some(p), _) => break Some(p.0),
                        (None, Some(a)) => region = plan.arms[a.0 as usize].region,
                        (None, None) => break None,
                    }
                }
            }
            (None, None) => None,
        }
    }

    /// Preserve literal authored IDs for URL references and for drag bindings
    /// when the template rewrites an SVG element's DOM ID.
    pub(super) fn exact_id(&self, i: u32) -> Option<String> {
        let plan = self.plan;
        let node = &plan.node(NodesId(i));
        if !self.refs && !NodeType::from_wire(node.node_type).is_some_and(|t| t.is_svg_element()) {
            return None;
        }
        node.bindings.iter().map(|b| plan.binding(b)).find_map(|b| {
            (b.kind == BindingKind::Prop && b.id == PropId::Id as u16)
                .then(|| style::literal(plan, plan.code(b.expr)))
                .flatten()
                .and_then(|v| v.as_str().map(str::to_string))
        })
    }
}

/// The `@keyframes` that animate `scale`: a pressed node plays their
/// `-exact-press` copies.
pub(super) fn press_keyframes(plan: &exact_plan::Plan) -> Vec<String> {
    plan.keyframes
        .iter()
        .filter(|k| {
            exact_motion::Keyframes::parse(plan.str(k.css))
                .is_ok_and(|f| f.css().contains("scale:") || f.css().contains("translate:"))
        })
        .map(|k| plan.str(k.name).to_string())
        .collect()
}

/// Whether a dynamic style row can reference an element (`url(#…)`).
pub(super) fn can_refer(plan: &exact_plan::Plan) -> bool {
    plan.bindings.iter().any(|b| {
        b.kind == BindingKind::Style
            && style::literal(plan, plan.code(b.expr)).is_none()
            && style::can_be(plan, plan.code(b.expr), &|v| v.contains("url("))
    })
}

/// Take the presence rows' declarations (LLP 1063) out of a node's static
/// CSS, returned in the order they were written.
pub(super) fn presence_decls(css: &mut String) -> String {
    let mut kept = String::new();
    let mut taken = String::new();
    for decl in css.split_inclusive(';') {
        // The drag timelines' too (LLP 1057.003 D2, D4): motion-glue.js
        // reads each from the element's own declaration.
        if super::motion::PRESENCE_ROWS
            .iter()
            .chain(&[
                "--exact-drag-timeline:",
                "--exact-animation-timeline:",
                // A synced animation's clock (LLP 1055.002), read the same way.
                "--exact-animation-clock:",
                "--exact-animation-range:",
                "--exact-timeline-scope:",
                "--exact-scroll-timeline:",
                // A browser timeline's consumer, which index.html's `[style*=…]` finds.
                "--exact-css-timeline:",
                "--exact-view-timeline:",
                // The press feedback's factor (LLP 1061) and haptic (LLP 1077
                // D14), which input-glue.js reads from the element's own style.
                "--exact-press:",
                "--exact-press-haptic:",
                // The pressed paint (LLP 1061 D7), read the same way.
                "--exact-press-translate:",
                "--exact-press-x:",
                "--exact-press-y:",
                "--exact-base-translated:",
                "--exact-press-timing:",
                "--exact-press-opacity:",
                "--exact-press-opacity-to:",
                "--exact-press-background-color:",
                "--exact-press-color:",
                // A fitted text's (LLP 1116.001 D1.4) and a symbol effect's (LLP 1077
                // D12), which navigation.js finds by them.
                "--exact-fit:",
                "--exact-symbol-effect:",
                // A hit slop's sides, which index.html's `[style*=…]` finds.
                "--exact-hit-slop-",
            ])
            .any(|p| decl.starts_with(p))
        {
            taken.push_str(decl);
        } else {
            kept.push_str(decl);
        }
    }
    *css = kept;
    taken
}

impl Em<'_> {
    /// Mount surfaces and extensions (LLP 1024, 1075.003.000), then settle
    /// constant values after mounting, as rt.js `drain` does for bindings.
    pub(crate) fn element_extras(
        &mut self,
        tag: &str,
        e: &str,
        attrs: &[(String, String)],
        props: &exact_kernel::SortedMap<String, String>,
    ) {
        for name in ["value", "scrollTop", "scrollLeft"] {
            if let Some(v) = props
                .get(name)
                .filter(|_| name != "value" || tag == "select")
            {
                let p = self.uses.rt("P");
                let v = serde_json::to_string(v).unwrap();
                let _ = write!(self.out, "{p}({e},\"{name}\",()=>{v});");
            }
        }
        if let Some(v) = sandboxed_source(tag, props) {
            let p = self.uses.rt("P");
            let v = serde_json::to_string(v).unwrap();
            let _ = write!(self.out, "{p}({e},\"src\",()=>{v});");
        }
        if tag == "canvas" {
            let cv = self.uses.rt("cv");
            let _ = write!(self.out, "{cv}({e});");
        }
        if tag.contains('-') {
            let _ = write!(self.out, "{}({e});", self.uses.rt("nm"));
        }
        if attrs.iter().any(|(k, _)| k == "data-hatch") {
            let _ = write!(self.out, "{}({e});", self.uses.rt("ht"));
        }
        // A context menu's popover (LLP 1021 §5.1), named by a literal.
        if attrs.iter().any(|(k, _)| k == "contextpopover") {
            let _ = write!(self.out, "{}({e});", self.uses.rt("cp"));
        }
    }

    /// A node's `data-*` words (LLP 1075.003 §3.3): one attribute per word,
    /// re-derived when the bound object changes (`dataset.js`).
    pub(crate) fn dataset(&mut self, e: &str, f: &str) {
        let (effect, ds) = (self.uses.rt("effect"), self.uses.rt("ds"));
        let _ = write!(self.out, "{effect}(()=>{ds}({e},({f})()));");
    }
}

/// The document's attribute rules (`host/web/src/document.rs`, `Walk::element`),
/// for a static prop: `(attributes, text content, extra CSS)`.
pub(super) fn attributes(
    element: &str,
    props: &exact_kernel::SortedMap<String, String>,
) -> (Vec<(String, String)>, Option<String>, String) {
    let mut attrs = Vec::new();
    let mut content = None;
    let mut css = String::new();
    for (name, value) in props {
        match name.as_str() {
            "scrollFollowEnd" | "scrollTop" | "scrollLeft" => {}
            // An authored `autofocus=false` is kept for the agent's tree
            // (the runner reports it); the attribute would mean true.
            "autofocus" => {
                if value == "true" {
                    attrs.push((name.clone(), String::new()));
                } else {
                    attrs.push(("data-autofocus".into(), "false".into()));
                }
            }
            // The app's own file (LLP 1069.002 D7), or a frame's page given
            // one (LLP 1020.001 D7): symbols.js shows it through an object
            // URL, media.js plays it (podcast F19); the browser has no `app:`.
            "src"
                if matches!(element, "img" | "video" | "audio") && value.starts_with("app:/")
                    || element == "iframe"
                        && exact_web::host::document::frame_app_source(value) =>
            {
                attrs.push(("data-app-src".into(), value.clone()));
            }
            "poster" if element == "video" && value.starts_with("app:/") => {
                attrs.push(("data-app-poster".into(), value.clone()));
            }
            // A package's scheme (LLP 1133.019 D11 item 1): pictures.js
            // asks its implementation; the browser never fetches it.
            "src" if element == "img" && package_scheme(value) => {
                attrs.push(("data-exact-picture".into(), value.clone()));
            }
            "src" if element == "img" && value.starts_with("symbol:") => {
                attrs.push((
                    name.clone(),
                    "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='16' height='16'/%3E".into(),
                ));
            }
            "text" => {
                if element != "canvas" && !value.is_empty() {
                    content = Some(value.clone());
                }
            }
            "data-action" => {
                attrs.push((name.clone(), value.clone()));
                css.push_str("touch-action:none;");
            }
            // A select's value is its options' (`element_extras`); a
            // `<progress>`'s is its attribute, without which it is
            // indeterminate (LLP 1116 D8).
            "value" => match element {
                "input" | "button" | "option" | "progress" => {
                    attrs.push((name.clone(), value.clone()))
                }
                "textarea" => content = Some(value.clone()),
                _ => {}
            },
            "checked" | "inert" | "disabled" | "readonly" | "multiple" => {
                if value == "true" {
                    attrs.push((name.clone(), String::new()));
                }
            }
            "autoplay"
            | "controls"
            | "loop"
            | "muted"
            | "playsinline"
            | "disablepictureinpicture"
            | "disableremoteplayback"
                if element == "video" || element == "audio" =>
            {
                if value == "true" {
                    attrs.push((name.clone(), String::new()));
                }
            }
            "href" if !exact_web::host::document::navigable(value) => {}
            "src" if sandboxed_source(element, props).is_some() => {}
            "src"
                if element == "iframe"
                    && !exact_web::host::document::frame_navigable(
                        value,
                        props.get("sandbox").map(String::as_str),
                    ) =>
            {
                attrs.push((name.clone(), "about:blank".into()));
            }
            _ => attrs.push((name.clone(), value.clone())),
        }
    }
    // Off the page's thread, so an arrival does not hold a frame (LLP 1145 D3).
    attrs.extend((element == "img").then(|| ("decoding".into(), "async".into())));
    // A role the element has natively is not restated, as the live host leaves it off.
    let implicit = exact_web::host::implicit_role(element, |name| {
        attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    });
    if implicit.is_some() {
        attrs.retain(|(k, v)| !(k == "role" && Some(v.as_str()) == implicit));
    }
    (attrs, content, css)
}

/// Whether `src` begins with a scheme a package may serve: a lowercase word
/// that is neither a web scheme nor a host's own (LLP 1133.019 D11 item 1;
/// the bake's `HOST_SCHEMES`).
fn package_scheme(src: &str) -> bool {
    let Some((scheme, _)) = src.split_once(':') else {
        return false;
    };
    scheme.starts_with(|c: char| c.is_ascii_lowercase())
        && scheme
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && ![
            "http", "https", "data", "blob", "app", "doc", "symbol", "asset", "file", "about",
        ]
        .contains(&scheme)
}

/// A frame's literal `data:` source (refused alone, shown in an opaque
/// sandbox) with no literal `sandbox`: a bound one decides it, so it is no
/// attribute (the frame shows about:blank) and rt.js `P` writes it
/// (`element_extras`), keeping it to decide again as the sandbox changes.
fn sandboxed_source<'a>(
    element: &str,
    props: &'a exact_kernel::SortedMap<String, String>,
) -> Option<&'a String> {
    use exact_web::host::document::{frame_navigable, navigable};
    props.get("src").filter(|v| {
        element == "iframe"
            && !props.contains_key("sandbox")
            && !navigable(v)
            && frame_navigable(v, Some(""))
    })
}

#[cfg(test)]
#[path = "rows_tests.rs"]
mod tests;
