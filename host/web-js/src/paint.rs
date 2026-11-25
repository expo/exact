//! Paint facts carried by templates and bindings. Isolation is always decided
//! on the instance tree by paint.js, never on the flattened template tree.
use exact_kernel::{paint_order, NodeFacts, NodeType, PropId, SortedMap, StyleId, StyleProps};
use exact_plan::{BindingKind, BindingsRow, Plan};

/// The CSS value, with NaN for a value outside the row's grammar.
pub const Z_INDEX: &str = r#"v=>{
  if(v==null)return null;
  if(typeof v==='string'){
    const t=v.trim();
    if(!t)return null;
    if(/^(auto|unset)$/i.test(t))return t.toLowerCase();
  }
  const n=typeof v==='number'?v:typeof v==='string'&&/^[+-]?\d+$/.test(v.trim())?Number(v):NaN;
  return Number.isInteger(n)&&n>=-2147483648&&n<=2147483647?Math.max(-2147483645,Math.min(2147483645,n)):NaN;
}"#;

const MOTION: [StyleId; 4] = [
    StyleId::Transition,
    StyleId::Animation,
    StyleId::LayoutTransition,
    StyleId::ExitAnimation,
];

/// `motion-words.js`, its one placeholder filled.
pub(super) fn motion_words() -> String {
    include_str!("../motion-words.js").replace(
        "MAX_LINEAR_STOPS",
        &exact_motion::easing::MAX_LINEAR_STOPS.to_string(),
    )
}

/// The transition grammar, its paint policy and the last curve covering layout.
#[allow(clippy::field_reassign_with_default)]
pub(super) fn transitions(words: &str) -> String {
    exact_motion::link_transitions();
    let properties: std::collections::BTreeMap<_, _> = exact_motion::Property::ALL
        .iter()
        .flat_map(|p| [p.name(), p.css_name()])
        .chain(["all", "border-color"])
        .chain(exact_motion::Property::TRANSITION_ONLY.map(|p| p.name()))
        .filter_map(|name| {
            let transition = exact_motion::Transitions::parse(name).ok()?;
            let mut style = StyleProps::default();
            style.transition = transition;
            style.mask.set(StyleId::Transition);
            Some((name, own(&style).policy))
        })
        .collect();
    include_str!("../transition-paint.js")
        .replace(
            "MAX_TRANSITIONS",
            &exact_motion::MAX_TRANSITIONS.to_string(),
        )
        .replace("WORDS", words)
        .replace("PROPERTIES", &serde_json::to_string(&properties).unwrap())
}

/// Lower the bound row before CSS rewrites the composed properties and springs.
pub(super) fn press_timing_map(parser: &str, sampler: &str, written: bool) -> String {
    let properties: std::collections::BTreeSet<_> = exact_web::css::PRESS_ROWS
        .into_iter()
        .map(|(_, p)| p.css_name())
        .collect();
    format!(
        r#"v=>{{
            const items={{}}, names={};
            const parsed=({parser})(v,false,(property,duration,delay,curve,spring,easing)=>{{
                const covered=property==null||property==='all'?names:names.filter(p=>p===property);
                if(!covered.length)return;
                let item;
                if(spring){{
                    const {{sample,coefficients}}={sampler};
                    const values=coefficients(spring), timing=values&&sample(values);
                    if(timing){{
                        const [seconds,curve]=timing;
                        item=[seconds*1000,delay*1000,curve];
                    }}
                }}else if(duration+delay>0){{
                    item=[duration*1000,delay*1000,easing(curve,true)];
                }}
                for(const name of covered)items[name]=item;
            }});
            if(!parsed)return {written}?'{{}}':null;
            return parsed.held||({written}&&v!=null)?JSON.stringify(items):null;
        }}"#,
        serde_json::json!(properties)
    )
}

fn animations(plan: &Plan, words: &str) -> String {
    let names: Vec<_> = plan
        .keyframes
        .iter()
        .filter(|k| {
            exact_motion::Keyframes::parse(plan.str(k.css)).is_ok_and(|f| {
                f.0.iter().any(|k| {
                    k.values.iter().any(|(p, _)| {
                        matches!(
                            p,
                            exact_motion::Property::Opacity
                                | exact_motion::Property::Translate
                                | exact_motion::Property::Scale
                                | exact_motion::Property::Rotate
                                | exact_motion::Property::Layout
                        )
                    })
                })
            })
        })
        .map(|k| plan.str(k.name))
        .collect();
    // Insert authored names last. A keyframes named MAX_ANIMATIONS must keep its name.
    include_str!("../animation-paint.js")
        .replace("MAX_ANIMATIONS", &exact_motion::MAX_ANIMATIONS.to_string())
        .replace("WORDS", words)
        .replace("NAMES", &serde_json::to_string(&names).unwrap())
}

#[derive(Clone, Copy)]
pub enum Parser {
    Transition,
    Animation,
}

impl Parser {
    pub(super) fn source(self, plan: &Plan, words: &str) -> String {
        match self {
            Self::Transition => transitions(words),
            Self::Animation => animations(plan, words),
        }
    }

    pub fn emit(self, plan: &Plan, uses: &mut crate::code::Uses) -> String {
        let words = uses.constant("$motionWords", motion_words);
        let name = match self {
            Self::Transition => "$parseTransition",
            Self::Animation => "$animationStacks",
        };
        uses.constant(name, || self.source(plan, &words))
    }
}

fn own(style: &StyleProps) -> paint_order::Own {
    paint_order::own_from(paint_order::Facts {
        style,
        props: &Default::default(),
        kind: NodeType::View,
        root: false,
        parent_display: None,
        beside_exclusion: false,
        holds_layout_transition: false,
    })
}

// The low bits match the server's paint summary. The higher bits are
// independent inputs: OR-ing a static fact and bound facts cannot clear a
// different row's contribution. paint.js reads this one compact vocabulary.
const POSITION: u32 = 1;
const STACK: u32 = 2;
const POLICY: u32 = 4;
const OUTSIDE: u32 = 8;
const ISOLATION: u32 = 64;
const ROOT: u32 = 128;
const LAYOUT: u32 = 256;
const ABSOLUTE: u32 = 512;
const WRAP: u32 = 1024;
const TEXT: u32 = 2048;
const FLEX: u32 = 4096;
const CONTROL: u32 = 8192;
const BUTTON: u32 = 16384;
const GLASS: u32 = 32768;
const DISABLED: u32 = 65536;
const DIM: u32 = 131072;

pub fn attributes(node: &NodeFacts<'_>, attrs: &mut SortedMap<String, String>) {
    let s = node.style;
    let mut authored = s.clone();
    for row in MOTION
        .into_iter()
        .chain([StyleId::ZIndex, StyleId::PressTranslate])
    {
        authored.mask.clear(row);
    }
    let mut policy = StyleProps::default();
    for row in MOTION
        .into_iter()
        .chain([StyleId::TintColor, StyleId::PressTranslate])
    {
        if s.mask.has(row) {
            policy.mask.set(row);
        }
    }
    policy.transition = s.transition.clone();
    policy.animation = s.animation.clone();
    let host = paint_order::own_from(paint_order::Facts {
        style: &policy,
        props: node.props,
        // A control's policy depends on three independently bound props.
        kind: if node.node_type == NodeType::Control {
            NodeType::View
        } else {
            node.node_type
        },
        root: node.is_root,
        parent_display: None,
        beside_exclusion: false,
        holds_layout_transition: false,
    });
    let button_style = node.props.str(PropId::ButtonStyle).unwrap_or("bordered");
    let mut bits = 0;
    for (bit, yes) in [
        (
            POSITION,
            node.is_root || s.position_type != exact_kernel::PositionType::Static,
        ),
        (STACK, own(&authored).stacks),
        (POLICY, host.policy),
        (OUTSIDE, host.outside),
        (ISOLATION, s.isolation == exact_kernel::Isolation::Isolate),
        (ROOT, node.is_root),
        (LAYOUT, s.mask.has(StyleId::LayoutTransition)),
        (
            ABSOLUTE,
            s.position_type == exact_kernel::PositionType::Absolute,
        ),
        (WRAP, s.wrap_flow == exact_kernel::WrapFlow::Both),
        (TEXT, node.node_type == NodeType::Text),
        (
            FLEX,
            matches!(
                s.display,
                exact_kernel::Display::Flex | exact_kernel::Display::Grid
            ),
        ),
        (CONTROL, node.node_type == NodeType::Control),
        (
            BUTTON,
            node.node_type == NodeType::Control && node.props.str(PropId::Type) == Some("button"),
        ),
        (
            GLASS,
            node.node_type == NodeType::Control && button_style.ends_with("glass"),
        ),
        (
            DISABLED,
            node.node_type == NodeType::Control && node.props.bool(PropId::Disabled) == Some(true),
        ),
        (
            DIM,
            node.node_type == NodeType::Control && !matches!(button_style, "bordered" | "gray"),
        ),
    ] {
        if yes {
            bits |= bit;
        }
    }
    attrs.insert("data-exact-f".into(), bits.to_string());
    if bits & ISOLATION != 0 {
        attrs.insert("data-exact-own-isolation".into(), String::new());
    }
    if let Some(z) = s.z_index.filter(|_| s.mask.has(StyleId::ZIndex)) {
        attrs.insert(
            "data-exact-zi".into(),
            z.clamp(-paint_order::Z_MAX, paint_order::Z_MAX).to_string(),
        );
    }
}

/// A pass is needed only when a non-root box can introduce a layer. Static
/// layers count: region arms and repeated copies still need instance decisions.
pub fn needed(plan: &Plan, parts: &[Option<exact_web::host::template::Parts>]) -> bool {
    parts.iter().enumerate().any(|(i, p)| {
        let Some(p) = p else { return false };
        let bits: u32 = p.props.get("data-exact-f").unwrap().parse().unwrap();
        if bits & OUTSIDE != 0 {
            return false;
        }
        (bits & ROOT == 0
            && (bits & (POSITION | STACK | POLICY | CONTROL | WRAP) != 0
                || p.props.contains_key("data-exact-zi")))
            || plan
                .nodes
                .get(i)
                .unwrap()
                .bindings
                .iter()
                .map(|b| plan.binding(b))
                .any(|b| {
                    crate::style::literal(plan, plan.code(b.expr)).is_none()
                        && binding(
                            plan,
                            NodeType::from_wire(plan.nodes.get(i).unwrap().node_type).unwrap(),
                            &b,
                            |_| String::new(),
                        )
                        .is_some()
                })
    })
}

/// The attribute and its value expression, with `v` the binding's value.
/// The tests are own_from's value tests, in the row's authored vocabulary.
#[allow(clippy::field_reassign_with_default)]
pub fn binding(
    plan: &Plan,
    kind: NodeType,
    b: &BindingsRow,
    parser: impl FnOnce(Parser) -> String,
) -> Option<(String, String)> {
    let bits = |value: String| Some((format!("data-exact-f-{}-{}", b.kind as u8, b.id), value));
    let flag = |bit: u32, test: &str| bits(format!("({test})?{bit}:0"));
    if b.kind == BindingKind::Prop {
        return match PropId::from_wire(b.id)? {
            PropId::Type if kind == NodeType::Control => flag(BUTTON, "v===\"button\""),
            PropId::ButtonStyle if kind == NodeType::Control => bits(format!(
                "(String(v??\"bordered\").endsWith(\"glass\")?{GLASS}:0)|(![\"bordered\",\"gray\"].includes(String(v??\"bordered\"))?{DIM}:0)")),
            PropId::Disabled if kind == NodeType::Control => flag(DISABLED, "v===true"),
            PropId::ImageSource if kind == NodeType::Image => flag(POLICY, "v!=null&&String(v).startsWith(\"symbol:\")"),
            PropId::BackgroundMaterial => flag(POLICY, "v!=null&&v!==\"none\""),
            PropId::NavigationPresentation => flag(POLICY, "v===\"modal\""),
            PropId::SemanticTag => flag(OUTSIDE, "v===\"dialog\""),
            PropId::Popover => flag(OUTSIDE, "v!=null"),
            _ => None,
        };
    }
    let id = StyleId::from_bit(b.id as u32)?;
    let test = match id {
        StyleId::PositionType => return bits(format!(
            "v==null||v===\"static\"?0:{POSITION}|(v===\"sticky\"?{STACK}:0)|(v===\"absolute\"?{ABSOLUTE}:0)")),
        StyleId::Display => return flag(FLEX, "v===\"flex\"||v===\"grid\""),
        StyleId::ZIndex => return Some(("data-exact-zi".into(), format!("(z=>Number.isInteger(z)?z:null)(({Z_INDEX})(v))"))),
        StyleId::WrapFlow => return flag(WRAP, "v===\"both\""),
        StyleId::TintColor if kind == NodeType::Image => return flag(POLICY, "v!=null"),
        StyleId::Isolation => return flag(ISOLATION | STACK, "v===\"isolate\""),
        StyleId::LayoutTransition => return flag(LAYOUT | POLICY, &format!("({})(v,false)?.held", parser(Parser::Transition))),
        // Exits still resolve names and keyframes at build time.
        StyleId::ExitAnimation => {
            let values = crate::presence::exit_animations(plan, plan.code(b.expr), false);
            let table = serde_json::to_string(&values.held.keys().collect::<Vec<_>>()).unwrap();
            return flag(POLICY, &format!("typeof v===\"string\"&&{table}.includes(v)"));
        }
        StyleId::Transition => {
            return flag(POLICY, &format!("({})(v,false)?.stacks", parser(Parser::Transition)));
        }
        StyleId::Animation => {
            return flag(POLICY, &format!("({})(v)", parser(Parser::Animation)));
        }
        // A number token's name (LLP 1121 D7) stands for its value: one that
        // varies may be below 1; a name no token has is unset, 1.
        StyleId::Opacity => {
            let tokens: Vec<String> = plan
                .theme
                .iter()
                .filter(|r| r.kind == 2)
                .map(|r| {
                    let value = if r.slices.len > 0 { "0" } else { plan.str(r.value).trim() };
                    format!("{}:{value}", serde_json::to_string(plan.str(r.name)).unwrap())
                })
                .collect();
            let test = if tokens.is_empty() {
                "v!=null&&Number(v)<1".to_string()
            } else {
                format!(
                    "v!=null&&(typeof v===\"string\"&&isNaN(v)?({{{}}}[v.trim()]??1)<1:Number(v)<1)",
                    tokens.join(",")
                )
            };
            return flag(STACK, &test);
        }
        StyleId::PressScale => "v!=null&&Number(v)!==1",
        StyleId::Perspective => "v!=null&&parseFloat(v)>0",
        StyleId::PressTranslate => return flag(POLICY, "v!=null&&String(v).trim()!==\"\"&&!/^(none|unset|initial|revert|revert-layer)$/i.test(String(v).trim())"),
        StyleId::Translate
        | StyleId::Scale
        | StyleId::Rotate
        | StyleId::Transform
        | StyleId::Filter
        | StyleId::BackdropFilter
        | StyleId::ClipPath
        | StyleId::MaskImage => "v!=null&&String(v).trim()!==\"none\"",
        StyleId::MixBlendMode => "v!=null&&v!==\"normal\"",
        _ => return None,
    };
    flag(STACK, test)
}

/// Specialize the mirror to the facts this plan can produce, including
/// bindings whose values are not known until a particular instance mounts.
pub fn runtime(plan: &Plan) -> String {
    let style = |id: StyleId| {
        plan.nodes
            .iter()
            .flat_map(|n| n.bindings.iter())
            .map(|b| plan.binding(b))
            .any(|b| b.kind == BindingKind::Style && b.id == id as u16)
    };
    let button = plan
        .nodes
        .iter()
        .any(|n| NodeType::from_wire(n.node_type) == Some(NodeType::Control));
    include_str!("../paint.js").replace(
        "const Z = true, FLOW = true, LAYOUT = true, BUTTON = true;",
        &format!(
            "const Z = {}, FLOW = {}, LAYOUT = {}, BUTTON = {button};",
            style(StyleId::ZIndex),
            style(StyleId::WrapFlow),
            style(StyleId::LayoutTransition)
        ),
    )
}
