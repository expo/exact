//! One test owns the process-wide look switches, as in `base_scale.rs`.
use exact_painted::{presenter::PainterChoice, Presenter};

#[test]
fn button_symbols_follow_the_buttons_inline_direction() {
    let mut source = String::from(
        "component App\n  view\n    column width=400 height=600 background-color=\"white\"\n",
    );
    let mut cases = Vec::new();
    for direction in ["ltr", "rtl"] {
        for column in [false, true] {
            for leading in [true, false] {
                let name = format!("{direction}-{column}-{leading}");
                let axis = if column {
                    " flex-direction=\"column\""
                } else {
                    ""
                };
                source.push_str(&format!(
                    "      box direction=\"{direction}\" width=400\n        button testId=\"{name}\" width=200 height=64{axis}\n"
                ));
                let symbol = "          image \"symbol:star\" -exact-tint-color=\"#ff0000\"\n";
                let title = "          text \"Save\" color=\"#0000ff\"\n";
                for child in if leading {
                    [symbol, title]
                } else {
                    [title, symbol]
                } {
                    source.push_str(child);
                }
                cases.push((name, direction, column, leading));
            }
        }
    }
    let plan = contract::compile(&source).unwrap();
    let mut failures = Vec::new();
    for look in ["android", "material", "neutral"] {
        match look {
            "material" => exact_painted::app::show_material_look(),
            "neutral" => {
                exact_painted::app::show_neutral_look();
                exact_kernel::style::metrics::set_column(
                    exact_kernel::style::metrics::Column::Neutral,
                );
            }
            _ => {}
        }
        let (mut p, error) = Presenter::boot_with(
            &plan.encode(),
            (),
            (400.0, 600.0),
            1.0,
            std::path::PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        let shot = p.frame();
        let k = p.host().kernel();
        for (name, direction, column, leading) in &cases {
            let node = k.node_by_key(k.find_by_test_id(name)[0]).unwrap();
            assert_eq!(node.node_type, exact_kernel::NodeType::Control);
            let f = node.frame;
            let bounds = |channel: usize| {
                let mut min = u32::MAX;
                let mut max = 0;
                for y in f.y as u32..(f.y + f.height) as u32 {
                    for x in f.x as u32..(f.x + f.width) as u32 {
                        let pixel = shot.pixel(x, y).unwrap();
                        let rgb = [pixel.red(), pixel.green(), pixel.blue()];
                        if rgb[channel] > 200
                            && rgb[(channel + 1) % 3] < 50
                            && rgb[(channel + 2) % 3] < 50
                        {
                            let pos = if *column { y } else { x };
                            min = min.min(pos);
                            max = max.max(pos);
                        }
                    }
                }
                assert!(min <= max, "{look} {name}: missing channel {channel}");
                (min, max)
            };
            let symbol = bounds(0);
            let title = bounds(2);
            let symbol_first = if *column || *direction == "ltr" {
                *leading
            } else {
                !leading
            };
            let ordered = if symbol_first {
                symbol.1 < title.0
            } else {
                title.1 < symbol.0
            };
            if !ordered {
                failures.push(format!("{look} {name}: symbol {symbol:?}, title {title:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
