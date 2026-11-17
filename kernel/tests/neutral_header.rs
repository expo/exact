//! Header defaults are process-wide; each first-use theme gets a fresh process.
use exact_kernel::style::{roles, theme};
use exact_kernel::{
    ColorValue, Kernel, NodeType, Op, PropId, PropValue, StyleId, StyleMask, StyleProps, StyleValue,
};

const NAMES: [&str; 3] = ["-exact-background", "-exact-label", "-exact-separator"];
const THEMES: [[&str; 3]; 3] = [
    [
        "light-dark(#112233, #223344)",
        "light-dark(#445566, #556677)",
        "light-dark(#778899, #8899aa)",
    ],
    [
        "light-dark(#aabbcc, #bbccdd)",
        "light-dark(#ddeeff, #ccddee)",
        "light-dark(#123456, #234567)",
    ],
    ["-exact-background", "-exact-label", "-exact-separator"],
];

fn route() -> Kernel {
    let mut k = Kernel::with_monospace();
    let mut own = StyleProps::default();
    for row in [
        StyleId::BackgroundColor,
        StyleId::TextColor,
        StyleId::BorderColorBottom,
    ] {
        own.set_dynamic(row, &StyleValue::Text("#abcdef".into()))
            .unwrap();
    }
    own.written = own.mask;
    let prop = |id, prop, value: &str| Op::SetProp {
        id,
        prop,
        value: PropValue::Str(value.into()),
    };
    k.apply(
        0,
        1,
        &[
            Op::CreateView {
                id: 1,
                node_type: NodeType::View,
            },
            Op::CreateView {
                id: 2,
                node_type: NodeType::View,
            },
            Op::CreateView {
                id: 3,
                node_type: NodeType::Text,
            },
            Op::CreateView {
                id: 4,
                node_type: NodeType::View,
            },
            prop(1, PropId::NavigationKey, "route"),
            prop(2, PropId::SemanticTag, "header"),
            prop(4, PropId::SemanticTag, "header"),
            Op::SetStyle {
                id: 1,
                patch: Box::new(own.clone()),
            },
            Op::SetStyle {
                id: 4,
                patch: Box::new(own),
            },
            Op::SetChildren {
                id: 1,
                children: vec![2, 4],
            },
            Op::SetChildren {
                id: 2,
                children: vec![3],
            },
            Op::AttachRoot { id: 1 },
        ],
    )
    .unwrap();
    k
}

fn colors(k: &Kernel, id: u32) -> [ColorValue; 3] {
    let style = k
        .node(id)
        .unwrap()
        .computed_style(StyleMask::of(StyleId::TextColor));
    [
        style.background_color.unwrap(),
        style.text_color,
        style.border_color_bottom.unwrap(),
    ]
}

#[test]
fn neutral_headers_resolve_the_current_theme() {
    const FIRST: &str = "EXACT_TEST_HEADER_FIRST";
    let Ok(first) = std::env::var(FIRST) else {
        let mut failures = Vec::new();
        for first in 0..3 {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "neutral_headers_resolve_the_current_theme",
                    "--nocapture",
                ])
                .env(FIRST, first.to_string())
                .output()
                .unwrap();
            if !output.status.success() {
                failures.push(format!(
                    "first theme {first}:\n{}\n{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
        return;
    };
    roles::set_column(roles::Column::Neutral);
    let first: usize = first.parse().unwrap();
    let sessions: Vec<_> = THEMES
        .iter()
        .enumerate()
        .map(|(index, values)| {
            roles::Roles::new(
                NAMES
                    .iter()
                    .zip(values)
                    .filter(|_| index != 2)
                    .map(|(name, value)| {
                        (
                            roles::role(name).unwrap(),
                            ColorValue::parse_light_dark(value).unwrap(),
                        )
                    })
                    .collect(),
            )
        })
        .collect();
    let mut kernels = Vec::new();
    for index in [first, (first + 1) % 3, (first + 2) % 3] {
        let entries = NAMES
            .iter()
            .zip(THEMES[index])
            .filter_map(|(name, value)| (index != 2).then_some((*name, value, true)));
        let _theme = theme::declarations(entries);
        let _roles = roles::bind(&sessions[index]);
        kernels.push((index, route()));
    }
    kernels.rotate_left(1);
    let identities = NAMES.map(|name| ColorValue::Role(roles::role(name).unwrap()));
    for (index, mut k) in kernels {
        let _roles = roles::bind(&sessions[index]);
        for dark in [false, true] {
            assert_eq!(
                colors(&k, 2).map(|c| c.resolve(dark)),
                identities.map(|c| c.resolve(dark)),
                "theme {index}, dark {dark}, initialized under {first}"
            );
            assert_eq!(
                k.node(3)
                    .unwrap()
                    .computed_row(StyleId::TextColor, |s| s.text_color)
                    .resolve(dark),
                identities[1].resolve(dark),
                "descendants inherit the header's ink"
            );
            assert_eq!(
                colors(&k, 4).map(|c| c.resolve(dark)),
                [exact_kernel::Color(0xabcd_efff); 3],
                "authored colours win"
            );
        }
        assert_eq!(colors(&k, 2), identities);
        k.apply(
            0,
            2,
            &[Op::ClearStyle {
                id: 4,
                mask: StyleMask::of(StyleId::BackgroundColor)
                    .union(StyleMask::of(StyleId::TextColor))
                    .union(StyleMask::of(StyleId::BorderColorBottom)),
                written: false,
            }],
        )
        .unwrap();
        assert_eq!(colors(&k, 4), identities, "removal restores defaults");
    }
}
