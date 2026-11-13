use super::*;

fn anim(text: &str, keyframes: &str) -> Animation {
    let mut a = Animations::parse(text).unwrap();
    let k = Keyframes::parse(keyframes).unwrap();
    a.resolve(|_| Some(&k));
    a.0.remove(0)
}

fn at(a: &Animation, t: f64, p: Property) -> Option<f64> {
    a.sample(t, p, Value::scalar(0.25)).map(|v| v.x)
}

fn close(a: Option<f64>, b: f64) {
    let a = a.expect("a value");
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}

#[test]
fn the_shorthand_assigns_parts_as_css_does() {
    let a = Animations::parse("pulse 1.2s ease-out 600ms infinite").unwrap();
    let p = &a.0[0];
    assert_eq!(p.name, "pulse");
    assert_eq!(p.duration, 1.2);
    assert_eq!(p.delay, 0.6);
    assert_eq!(p.easing, Easing::EaseOut);
    assert!(p.iterations.is_infinite());
    // A keyword that could be another longhand's goes there first; the name
    // is what is left, wherever it stands.
    let b = Animations::parse("ease ease 1s alternate both paused 3").unwrap();
    let q = &b.0[0];
    assert_eq!((q.name.as_str(), q.easing.clone()), ("ease", Easing::Ease));
    assert_eq!(q.direction, Direction::Alternate);
    assert_eq!(q.fill, FillMode::Both);
    assert!(q.paused);
    assert_eq!(q.iterations, 3.0);
    assert_eq!(Animations::parse("none").unwrap(), Animations::NONE);
    assert_eq!(Animations::parse("a 1s, b 2s").unwrap().0.len(), 2);
    assert!(Animations::parse("a 1s 2s 3s").is_err());
    assert!(Animations::parse("a -exact-spring(100, 10, 1) 1s").is_err());
    assert!(Animations::parse("a -1s").is_err());
    assert!(Animations::parse("a b").is_err());
}

#[test]
fn the_shorthand_round_trips_through_css() {
    let a = Animations::parse(
        "draw 600ms ease-out both, breathe 1.2s cubic-bezier(0, 0, 0.58, 1) -300ms infinite paused",
    )
    .unwrap();
    let again = Animations::parse(&a.css()).unwrap();
    assert_eq!(a, again);
}

#[test]
fn keyframes_parse_merge_and_order() {
    let k =
        Keyframes::parse("to{r:9px;opacity:0}from{r:3;opacity:.5}50%,75%{opacity:0.2}").unwrap();
    let offsets: Vec<f64> = k.0.iter().map(|f| f.offset).collect();
    assert_eq!(offsets, vec![0.0, 0.5, 0.75, 1.0]);
    assert_eq!(k.properties(), vec![Property::Opacity, Property::R]);
    // Equal offsets and timing functions merge; a later value wins.
    let m = Keyframes::parse("0%{opacity:1}0%{opacity:0;r:2}").unwrap();
    assert_eq!(m.0.len(), 1);
    assert_eq!(
        m.0[0].values,
        vec![
            (Property::Opacity, Value::scalar(0.0)),
            (Property::R, Value::scalar(2.0))
        ]
    );
    assert!(Keyframes::parse("from{color:blurple}").is_err());
    assert!(Keyframes::parse("from{color:hsl(0 100% 50%)}").is_ok());
    assert!(Keyframes::parse("from{color:reddish}").is_err());
    assert!(Keyframes::parse("from{height:3px}").is_err());
    assert!(Keyframes::parse("120%{opacity:1}").is_err());
    assert_eq!(Keyframes::parse(&k.css()).unwrap(), k);
    assert_eq!(
        k.css(),
        "0%{r:3px;opacity:0.5;}50%{opacity:0.2;}75%{opacity:0.2;}100%{r:9px;opacity:0;}"
    );
}

#[test]
fn phases_and_fill_modes() {
    let k = "from{opacity:0}to{opacity:1}";
    let none = anim("a 1s linear 0.5s", k);
    assert_eq!(
        at(&none, 0.25, Property::Opacity),
        None,
        "delay, no backwards fill"
    );
    close(at(&none, 1.0, Property::Opacity), 0.5);
    assert_eq!(
        at(&none, 1.5, Property::Opacity),
        None,
        "ended, no forwards fill"
    );
    let both = anim("a 1s linear 0.5s both", k);
    close(at(&both, 0.25, Property::Opacity), 0.0);
    close(at(&both, 9.0, Property::Opacity), 1.0);
    // A negative delay starts partway through.
    let early = anim("a 1s linear -0.25s", k);
    close(at(&early, 0.0, Property::Opacity), 0.25);
    assert_eq!(early.end_time(), 0.75);
}

#[test]
fn iterations_and_directions() {
    let k = "from{opacity:0}to{opacity:1}";
    let alt = anim("a 1s linear 3 alternate forwards", k);
    close(at(&alt, 0.25, Property::Opacity), 0.25);
    close(at(&alt, 1.25, Property::Opacity), 0.75);
    close(at(&alt, 2.25, Property::Opacity), 0.25);
    // After three iterations the last (forward) one fills at its end.
    close(at(&alt, 5.0, Property::Opacity), 1.0);
    let rev = anim("a 1s linear reverse", k);
    close(at(&rev, 0.25, Property::Opacity), 0.75);
    let ar = anim("a 1s linear 2 alternate-reverse forwards", k);
    close(at(&ar, 0.25, Property::Opacity), 0.75);
    close(at(&ar, 1.25, Property::Opacity), 0.25);
    close(at(&ar, 3.0, Property::Opacity), 1.0);
    // A fractional count ends partway through an iteration.
    let half = anim("a 1s linear 1.5 forwards", k);
    close(at(&half, 9.0, Property::Opacity), 0.5);
    // Infinite never ends.
    let inf = anim("a 1s linear infinite", k);
    assert!(inf.end_time().is_infinite());
    close(at(&inf, 1000.25, Property::Opacity), 0.25);
}

#[test]
fn implicit_endpoints_take_the_underlying_value() {
    // Only `to` is declared: `from` is the underlying 0.25.
    let a = anim("a 1s linear", "to{opacity:1}");
    close(at(&a, 0.0, Property::Opacity), 0.25);
    close(at(&a, 0.5, Property::Opacity), 0.625);
    // A property the rule does not name is not animated.
    assert_eq!(at(&a, 0.5, Property::R), None);
}

#[test]
fn the_timing_function_applies_per_keyframe_interval() {
    let k = "0%{opacity:0}50%{opacity:1;animation-timing-function:linear}100%{opacity:0}";
    let a = anim("a 2s ease-in", k);
    // First half: ease-in over the interval.
    close(at(&a, 0.5, Property::Opacity), Easing::EaseIn.progress(0.5));
    // Second half: the keyframe's own linear.
    close(at(&a, 1.5, Property::Opacity), 0.5);
}

/// Two blocks at 100%, as Chrome samples them: the last holds after the end,
/// and a block naming the animation's own timing function is one keyframe
/// with a block naming none, the later value winning.
#[test]
fn repeated_keyframes_at_one_offset_end_on_the_last() {
    let k = |easing: &str| {
        format!(
            "from{{opacity:0}}to{{opacity:0.2;animation-timing-function:{easing}}}to{{opacity:0.9}}"
        )
    };
    let apart = anim("a 1s linear forwards", &k("ease-in"));
    close(at(&apart, 0.5, Property::Opacity), 0.1);
    close(at(&apart, 1.0, Property::Opacity), 0.9);
    close(at(&apart, 2.0, Property::Opacity), 0.9);
    let merged = anim("a 1s linear forwards", &k("linear"));
    close(at(&merged, 0.5, Property::Opacity), 0.45);
    close(at(&merged, 1.0, Property::Opacity), 0.9);
}

#[test]
fn zero_duration_jumps_to_the_end_with_forwards_fill() {
    let k = "from{opacity:0}to{opacity:1}";
    let a = anim("a 0s forwards", k);
    close(at(&a, 0.0, Property::Opacity), 1.0);
    let b = anim("a 0s", k);
    assert_eq!(at(&b, 0.0, Property::Opacity), None);
}

#[test]
fn the_benchmark_draw_in_and_pulse() {
    let draw = anim(
        "draw 600ms ease-out both",
        "from{stroke-dashoffset:1}to{stroke-dashoffset:0}",
    );
    close(at(&draw, 0.0, Property::StrokeDashoffset), 1.0);
    close(at(&draw, 0.6, Property::StrokeDashoffset), 0.0);
    close(
        at(&draw, 0.3, Property::StrokeDashoffset),
        1.0 - Easing::EaseOut.progress(0.5),
    );
    let pulse = anim(
        "breathe 1200ms ease-out 600ms infinite",
        "from{r:3;opacity:.5}to{r:9;opacity:0}",
    );
    assert_eq!(at(&pulse, 0.3, Property::R), None, "waits for the draw-in");
    close(at(&pulse, 0.6, Property::R), 3.0);
    close(
        at(&pulse, 1.2, Property::R),
        3.0 + 6.0 * Easing::EaseOut.progress(0.5),
    );
    close(at(&pulse, 1.8, Property::R), 3.0);
}

#[test]
fn font_rows_parse_and_write_back_as_css() {
    let k = Keyframes::parse(
        "from{font-weight:bold;font-stretch:condensed;font-variation-settings:'wght' 300, \"GRAD\" 0}\
         to{font-weight:900;font-stretch:125%;font-variation-settings:'GRAD' 100,'wght' 800}",
    )
    .unwrap();
    assert_eq!(
        k.css(),
        "0%{font-weight:700;font-stretch:75%;font-variation-settings:\"wght\" 300,\"GRAD\" 0;}\
         100%{font-weight:900;font-stretch:125%;font-variation-settings:\"GRAD\" 100,\"wght\" 800;}"
    );
    assert_eq!(Keyframes::parse(&k.css()).unwrap(), k);
    let wght = u32::from_be_bytes(*b"wght");
    let axes: Vec<(u32, f64)> = (k.0[1].values.iter())
        .filter(|(p, _)| *p == Property::FontVariationSettings)
        .map(|(_, v)| (v.axis_tag(), v.x))
        .collect();
    assert_eq!(axes[1], (wght, 800.0));
}

#[test]
fn keyframes_that_name_different_axes_are_refused() {
    for bad in [
        "from{font-variation-settings:'wght' 300}to{font-variation-settings:'wdth' 80}",
        "to{font-variation-settings:normal}",
        "to{font-weight:bolder}",
        "to{font-weight:1001}",
    ] {
        assert!(Keyframes::parse(bad).is_err(), "{bad}");
    }
}

#[test]
fn a_refusal_states_the_rule_in_words() {
    let longhand = |name: &str, value: &str| {
        Animation::default()
            .set_longhand(name, value)
            .unwrap_err()
            .to_string()
    };
    for (message, says) in [
        (
            longhand("animation-duration", "1"),
            "`1` is not a time: a time is a number with `s` or `ms`",
        ),
        (
            longhand("animation-iteration-count", "-1"),
            "a number 0 or more, or `infinite`",
        ),
        (
            longhand("animation-direction", "bogus"),
            "normal, reverse, alternate or alternate-reverse",
        ),
        (
            longhand("animation-timing-function", "-exact-spring()"),
            "a spring is a transition's timing function",
        ),
        // The longhand takes no time: `1s` is refused as an easing alone.
        (
            longhand("animation-timing-function", "1s"),
            "`1s` is not an easing: linear, ease,",
        ),
        (
            Animations::parse("a 1s steps(0)").unwrap_err().to_string(),
            "`steps()` takes at least one step",
        ),
        (
            Keyframes::parse("from{opacity:x}").unwrap_err().to_string(),
            "`x` is not a value `opacity` takes",
        ),
    ] {
        assert!(message.contains(says), "{message}");
        assert!(!message.contains("neither a time"), "{message}");
    }
}

/// Each refusal says which limit broke, for the shorthand and a keyframes
/// body alike.
#[test]
fn a_shape_refusal_says_which_limit_broke() {
    for (text, says) in [
        (
            "breathe 1s 2s 3s",
            "has a third time: an animation takes a duration, then a delay",
        ),
        (
            "spin 1s other",
            "has a second name, `other`, beside `spin`: an animation has one",
        ),
        (
            "a 1s ease linear",
            "gives an easing twice, `linear` the second time",
        ),
        (
            "a 1s forwards none",
            "gives a fill mode twice, `none` the second time",
        ),
        ("a 1s @", "`@` in `a 1s @` is no part of an animation"),
        (
            "a 1s inherit",
            "a name, which is an identifier other than `initial`",
        ),
        (
            "spin default",
            "`default` in `spin default` is no part of an animation",
        ),
        ("breathe 1.2.3s", "`1.2.3s` is not a time"),
        ("-1s breathe", "a duration is 0 or more"),
        ("breathe 1e999s", "an animation's duration is finite"),
        ("breathe 1s 1e999s", "an animation's delay is finite"),
        (
            "a 1s,",
            "has an empty entry: each comma separates two animations",
        ),
        (
            "a 1s cubic-bezier(1, 2)",
            "`cubic-bezier()` takes four numbers",
        ),
    ] {
        let message = Animations::parse(text).unwrap_err().to_string();
        assert!(message.contains(says), "{text}: {message}");
    }
    for (text, says) in [
        (
            "120%{opacity:1}",
            "`120%` is not a keyframe selector: `from`, `to` or a percentage",
        ),
        (
            "from{opacity 1}",
            "`opacity 1` in a keyframe is not `property: value`",
        ),
        (
            "from{height:3px}",
            "`height` is not a property keyframes animate",
        ),
        ("from opacity:1", "has no `{`"),
        ("from{opacity:1", "has no closing `}`"),
        (
            "from{opacity:1e999}",
            "an animation's keyframe value is finite",
        ),
        (
            "from{font-variation-settings:normal}",
            "`normal` names no axes",
        ),
        (
            "from{font-variation-settings:wght}",
            "`wght` is not a value `font-variation-settings` takes",
        ),
    ] {
        let message = Keyframes::parse(text).unwrap_err().to_string();
        assert!(message.contains(says), "{text}: {message}");
        assert!(!message.contains("transition"), "{text}: {message}");
    }
}

/// Limits a keyframes list breaks by count: too many keyframes, too many axes.
#[test]
fn a_count_refusal_names_its_own_limit() {
    let many: String = (0..33).map(|i| format!("{i}%{{opacity:1}}")).collect();
    let message = Keyframes::parse(&many).unwrap_err().to_string();
    assert_eq!(message, "keyframes hold at most 32 keyframes");
    let nine: Vec<String> = (0..9).map(|i| format!("\"ax{i:02}\" 1")).collect();
    let message = Keyframes::parse(&format!(
        "from{{font-variation-settings:{}}}",
        nine.join(", ")
    ))
    .unwrap_err()
    .to_string();
    assert!(message.contains("names more than 8 axes"), "{message}");
}

/// #49: every word but the name reads in any ASCII case, as CSS's do; the
/// name, a `<custom-ident>`, keeps its case (CSS Animations 1 §3, §4.10).
#[test]
fn the_shorthand_and_longhands_read_in_any_ascii_case() {
    for (text, lower) in [
        (
            "Spin 1S EASE-IN 200MS INFINITE Alternate-Reverse BOTH Paused",
            "Spin 1s ease-in 200ms infinite alternate-reverse both paused",
        ),
        (
            "Spin 1s Cubic-Bezier(0, 0, 1, 1)",
            "Spin 1s cubic-bezier(0, 0, 1, 1)",
        ),
        ("Spin 1s STEPS(3, Jump-None)", "Spin 1s steps(3, jump-none)"),
        (
            "Spin 1s Reverse Forwards Running",
            "Spin 1s reverse forwards running",
        ),
        ("NONE", "none"),
    ] {
        let parsed = Animations::parse(text);
        assert!(parsed.is_ok(), "{text}: {parsed:?}");
        assert_eq!(parsed, Animations::parse(lower), "{text}");
    }
    // A keyword in another case is the keyword, not the name: `1s EASE`
    // names nothing, as `1s ease` does, and `spin 1s LINEAR` is `spin`'s.
    assert_eq!(Animations::parse("1s EASE").unwrap(), Animations::NONE);
    let spin = Animations::parse("spin 1s LINEAR").unwrap();
    assert_eq!(
        (spin.0[0].name.as_str(), &spin.0[0].easing),
        ("spin", &Easing::Linear)
    );
    // A keyword whose part is given is the name, as written.
    let named = Animations::parse("ease EASE 1s").unwrap();
    assert_eq!(named.0[0].name, "EASE");
    // `none` is none in any case; a CSS-wide keyword is no name in any case.
    assert_eq!(
        Animations::parse("forwards NONE 1s").unwrap(),
        Animations::NONE
    );
    for text in ["INHERIT 1s", "Initial 1s", "DEFAULT 1s"] {
        assert!(Animations::parse(text).is_err(), "{text}");
    }
    // Rust reads `inf` as a number; CSS does not, so it is a name.
    assert_eq!(Animations::parse("1s INF").unwrap().0[0].name, "INF");
    // The longhands, likewise.
    let mut a = Animation::default();
    for (name, value) in [
        ("animation-name", "Spin"),
        ("animation-duration", "2S"),
        ("animation-delay", "100MS"),
        ("animation-timing-function", "Ease-Out"),
        ("animation-iteration-count", "INFINITE"),
        ("animation-direction", "ALTERNATE"),
        ("animation-fill-mode", "Backwards"),
        ("animation-play-state", "PAUSED"),
    ] {
        a.set_longhand(name, value)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
    }
    assert_eq!(
        Animations(vec![a]).css(),
        "2s ease-out 0.1s infinite alternate backwards paused Spin"
    );
    let mut none = Animation::default();
    none.set_longhand("animation-name", "NONE").unwrap();
    assert_eq!(none.name, "none");
    // A keyframes selector, too.
    assert_eq!(
        Keyframes::parse("FROM{opacity:0}To{opacity:1}").unwrap(),
        Keyframes::parse("from{opacity:0}to{opacity:1}").unwrap()
    );
    // And a keyframe value's units and keywords.
    assert_eq!(
        Keyframes::parse("from{translate:10PX 50%;rotate:45DEG;r:2Px;font-weight:BOLD;font-stretch:Condensed;box-shadow:NONE}to{font-weight:Normal;box-shadow:0px 2PX 4Px Red}").unwrap(),
        Keyframes::parse("from{translate:10px 50%;rotate:45deg;r:2px;font-weight:bold;font-stretch:condensed;box-shadow:none}to{font-weight:normal;box-shadow:0px 2px 4px red}").unwrap()
    );
    assert!(Keyframes::parse("from{font-variation-settings:NORMAL}")
        .unwrap_err()
        .to_string()
        .contains("names no axes"));
}

/// The shorthand's grammar and the module-scope pass read a word through
/// one [`Slots`]: the first free part it can be, and the name last.
#[test]
fn slots_give_each_word_the_first_free_part_it_can_be() {
    let mut slots = Slots::default();
    let roles: Vec<_> = ["EASE", "Ease", "2", "3S", "1s", "4s"]
        .into_iter()
        .map(|w| slots.take(w))
        .collect();
    assert_eq!(
        roles,
        [
            Some(Role::Easing),
            Some(Role::Name),
            Some(Role::Count),
            Some(Role::Time),
            Some(Role::Time),
            None
        ]
    );
    assert_eq!(Slots::default().take("1.2.3s"), None);
    assert!(is_easing("Spring(1, 2, 3)") && is_easing("Steps(") && !is_easing("spin"));
}
