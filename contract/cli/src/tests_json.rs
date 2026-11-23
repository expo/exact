//! The tests as JSON for the driver (LLP 1012).

use contract_syntax::{Expr, Step, TapForm, TestDecl};

/// A JSON string.
fn q(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// The tests as JSON for the driver: `[{"name":…,"steps":[{"op":…}]}]`,
/// written by hand — no serde anywhere in the runtime (LLP 1012).
pub fn tests_json(tests: &[TestDecl]) -> String {
    let mut s = String::from("[");
    for (ti, t) in tests.iter().enumerate() {
        if ti > 0 {
            s.push(',');
        }
        s.push_str("{\"name\":");
        q(&t.name, &mut s);
        if !t.hosts.is_empty() {
            s.push_str(",\"hosts\":[");
            for (i, h) in t.hosts.iter().enumerate() {
                if i > 0 {
                    s.push(',');
                }
                q(h, &mut s);
            }
            s.push(']');
        }
        s.push_str(",\"steps\":");
        steps_json(&t.steps, &mut s);
        s.push('}');
    }
    s.push(']');
    s
}

/// A list of steps as JSON, each with its line.
fn steps_json(steps: &[Step], s: &mut String) {
    s.push('[');
    for (si, step) in steps.iter().enumerate() {
        if si > 0 {
            s.push(',');
        }
        step_json(step, s);
    }
    s.push(']');
}

/// A drag leg's `during` ops, when it names any.
fn during_json(during: &[String], s: &mut String) {
    if during.is_empty() {
        return;
    }
    s.push_str(",\"during\":[");
    for (i, op) in during.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        q(op, s);
    }
    s.push(']');
}

/// One step as JSON: `{"op":…,…,"line":n}`.
fn step_json(step: &Step, s: &mut String) {
    let line = step.span().line;
    match step {
        Step::Scroll { target, .. } => {
            s.push_str("{\"op\":\"scroll\",\"target\":");
            q(target, s);
        }
        Step::Tap {
            target,
            form,
            modifiers,
            ..
        } => {
            s.push_str("{\"op\":\"tap\",\"target\":");
            q(target, s);
            s.push_str(",\"form\":");
            match form {
                TapForm::Press => s.push_str("\"press\""),
                TapForm::Hover => s.push_str("\"hover\""),
                TapForm::Dblclick => s.push_str("\"dblclick\""),
                TapForm::Contextmenu => s.push_str("\"contextmenu\""),
                TapForm::History(delta) => s.push_str(&format!("\"history\",\"history\":{delta}")),
                TapForm::Into(key) => {
                    s.push_str("\"into\",\"key\":");
                    q(key, s);
                }
                TapForm::Pinch { scale, at } => {
                    s.push_str(&format!("\"pinch\",\"scale\":{scale}"));
                    if let Some((x, y)) = at {
                        s.push_str(&format!(",\"at\":[{x},{y}]"));
                    }
                }
                TapForm::Wheel { dx, dy } => {
                    s.push_str(&format!("\"wheel\",\"wheel\":[{dx},{dy}]"))
                }
                // @ref LLP 1098 D10 — the platform's action, never a press.
                TapForm::MediaSession { action, seconds } => {
                    s.push_str("\"mediasession\",\"action\":");
                    q(action, s);
                    if let Some(n) = seconds {
                        s.push_str(&format!(",\"seconds\":{n}"));
                    }
                }
            }
            if !modifiers.is_empty() {
                s.push_str(",\"modifiers\":");
                q(modifiers, s);
            }
        }
        Step::Drag {
            target,
            dx,
            dy,
            to,
            from,
            mouse,
            into,
            press,
            over,
            hold,
            during,
            modifiers,
            path,
            ..
        } => {
            s.push_str("{\"op\":\"drag\",\"target\":");
            q(target, s);
            match to {
                // LLP 1094 D12: the driver's `drag to`.
                Some((to, at)) => {
                    s.push_str(",\"to\":");
                    q(to, s);
                    if let Some((x, y)) = at {
                        s.push_str(&format!(",\"at\":[{x},{y}]"));
                    }
                }
                None => s.push_str(&format!(",\"dx\":{dx},\"dy\":{dy}")),
            }
            if let Some((x, y)) = from {
                s.push_str(&format!(",\"from\":[{x},{y}]"));
            }
            if *mouse {
                s.push_str(",\"mouse\":true");
            }
            if let Some(into) = into {
                s.push_str(",\"into\":");
                q(into, s);
            }
            for (name, ms) in [("press", press), ("over", over), ("hold", hold)] {
                if let Some(ms) = ms {
                    s.push_str(&format!(",\"{name}\":{ms}"));
                }
            }
            during_json(during, s);
            if !modifiers.is_empty() {
                s.push_str(",\"modifiers\":");
                q(modifiers, s);
            }
            if !path.is_empty() {
                s.push_str(",\"path\":[");
                for (i, leg) in path.iter().enumerate() {
                    if i > 0 {
                        s.push(',');
                    }
                    s.push_str(&format!("{{\"dx\":{},\"dy\":{}", leg.dx, leg.dy));
                    for (name, ms) in [("over", leg.over), ("hold", leg.hold)] {
                        if let Some(ms) = ms {
                            s.push_str(&format!(",\"{name}\":{ms}"));
                        }
                    }
                    during_json(&leg.during, s);
                    s.push('}');
                }
                s.push(']');
            }
        }
        Step::Size { width, height, .. } => {
            s.push_str(&format!(
                "{{\"op\":\"size\",\"width\":{width},\"height\":{height}"
            ));
        }
        Step::Epoch { value, .. } => {
            s.push_str("{\"op\":\"epoch\",\"value\":");
            q(value, s);
        }
        Step::TimeZone { zone, .. } => {
            s.push_str("{\"op\":\"time-zone\",\"value\":");
            q(zone, s);
        }
        Step::At { location, .. } => {
            s.push_str("{\"op\":\"at\",\"value\":");
            q(location, s);
        }
        Step::Locale { tag, .. } => {
            s.push_str("{\"op\":\"locale\",\"value\":");
            q(tag, s);
        }
        Step::Prefer { name, value, .. } => {
            s.push_str("{\"op\":\"prefer\",\"name\":");
            q(name, s);
            s.push_str(",\"value\":");
            q(value, s);
        }
        Step::Seed { seed, .. } => {
            s.push_str(&format!("{{\"op\":\"seed\",\"value\":{seed}"));
        }
        Step::SignedIn { key, account, .. } => {
            s.push_str("{\"op\":\"signed-in\",\"key\":");
            q(key, s);
            s.push_str(",\"account\":");
            s.push_str(account);
        }
        Step::Stored {
            name,
            value,
            window,
            ..
        } => {
            s.push_str("{\"op\":\"stored\",\"name\":");
            q(name, s);
            if *window {
                s.push_str(",\"window\":true");
            }
            s.push_str(",\"json\":");
            q(value, s);
        }
        Step::Fresh { seed, .. } => {
            s.push_str("{\"op\":\"fresh\",\"seed\":");
            match seed {
                Some(path) => q(path, s),
                None => s.push_str("null"),
            }
        }
        Step::Type {
            target,
            text,
            append,
            ..
        } => {
            s.push_str("{\"op\":\"type\",\"target\":");
            q(target, s);
            s.push_str(",\"text\":");
            q(text, s);
            s.push_str(&format!(",\"append\":{append}"));
        }
        Step::Reload { .. } => s.push_str("{\"op\":\"reload\""),
        Step::Close { .. } => s.push_str("{\"op\":\"close\""),
        Step::Back { .. } => s.push_str("{\"op\":\"back\""),
        Step::BeforeData { .. } => s.push_str("{\"op\":\"before-data\""),
        Step::FailFetch { prefix, times, .. } => {
            s.push_str("{\"op\":\"fail-fetch\",\"prefix\":");
            q(prefix, s);
            match times {
                Some(n) => s.push_str(&format!(",\"times\":{n}")),
                None => s.push_str(",\"times\":null"),
            }
        }
        Step::AnswerFetch {
            prefix,
            method,
            response,
            ..
        } => {
            s.push_str("{\"op\":\"answer-fetch\",\"prefix\":");
            q(prefix, s);
            if let Some(method) = method {
                s.push_str(",\"method\":");
                q(method, s);
            }
            s.push_str(",\"response\":");
            s.push_str(response);
        }
        Step::AnswerSource { source, value, .. } => {
            s.push_str("{\"op\":\"answer-source\",\"source\":");
            q(source, s);
            s.push_str(",\"json\":");
            q(value, s);
        }
        Step::PassFetch { prefix, .. } => {
            s.push_str("{\"op\":\"pass-fetch\",\"prefix\":");
            q(prefix, s);
        }
        Step::DropStream { prefix, .. } => {
            s.push_str("{\"op\":\"drop-stream\",\"prefix\":");
            q(prefix, s);
        }
        // @ref LLP 1116.006 D8 — the body as JSON text, the persona as `as`.
        Step::Request {
            method,
            path,
            body,
            persona,
            status,
            ..
        } => {
            s.push_str("{\"op\":\"request\",\"method\":");
            q(method, s);
            s.push_str(",\"path\":");
            q(path, s);
            for (key, value) in [("body", body), ("as", persona)] {
                s.push_str(&format!(",\"{key}\":"));
                match value {
                    Some(v) => q(v, s),
                    None => s.push_str("null"),
                }
            }
            // LLP 1116.010 D3: only when written, so a plain request's line is as it was.
            if let Some(status) = status {
                s.push_str(&format!(",\"expect\":{status}"));
            }
        }
        Step::Resize { width, height, .. } => {
            s.push_str(&format!(
                "{{\"op\":\"resize\",\"width\":{width},\"height\":{height}"
            ));
        }
        Step::Key {
            target,
            key,
            phase,
            duration,
            ..
        } => {
            s.push_str("{\"op\":\"key\",\"target\":");
            q(target, s);
            s.push_str(",\"key\":");
            q(key, s);
            if let Some(phase) = phase {
                s.push_str(",\"phase\":");
                q(phase, s);
            }
            if let Some(ms) = duration {
                s.push_str(&format!(",\"for\":{ms}"));
            }
        }
        Step::Pick { target, paths, .. } => {
            s.push_str("{\"op\":\"pick\",\"target\":");
            q(target, s);
            s.push_str(",\"paths\":[");
            for (i, path) in paths.iter().enumerate() {
                if i > 0 {
                    s.push(',');
                }
                q(path, s);
            }
            s.push(']');
        }
        Step::Authorize { cancel, .. } => {
            s.push_str(&format!("{{\"op\":\"authorize\",\"cancel\":{cancel}"));
        }
        Step::Presence { answer, .. } => {
            s.push_str("{\"op\":\"presence\",\"answer\":");
            q(answer, s);
        }
        Step::Clipboard {
            target, edit, text, ..
        } => {
            s.push_str("{\"op\":\"clipboard\",\"target\":");
            q(target, s);
            s.push_str(",\"edit\":");
            q(edit, s);
            s.push_str(",\"text\":");
            q(text, s);
        }
        Step::Background { ms, .. } => {
            s.push_str(&format!("{{\"op\":\"background\",\"ms\":{ms}"));
        }
        Step::Clock { arg, .. } => {
            s.push_str("{\"op\":\"clock\",\"arg\":");
            q(arg, s);
        }
        Step::Screenshot { path, .. } => {
            s.push_str("{\"op\":\"screenshot\",\"path\":");
            q(path, s);
        }
        Step::ExpectRefusal { target, .. } => {
            s.push_str("{\"op\":\"expect-refusal\",\"target\":");
            q(target, s);
        }
        Step::ExpectTree {
            target, present, ..
        } => {
            s.push_str("{\"op\":\"expect-tree\",\"target\":");
            q(target, s);
            s.push_str(&format!(",\"present\":{present}"));
        }
        Step::ExpectDisabled {
            target, disabled, ..
        } => {
            s.push_str("{\"op\":\"expect-disabled\",\"target\":");
            q(target, s);
            s.push_str(&format!(",\"disabled\":{disabled}"));
        }
        Step::ExpectSample {
            target,
            x,
            y,
            color,
            tolerance,
            ..
        } => {
            s.push_str("{\"op\":\"expect-sample\",\"target\":");
            q(target, s);
            s.push_str(&format!(
                ",\"x\":{x},\"y\":{y},\"tolerance\":{tolerance},\"color\":"
            ));
            q(color, s);
        }
        Step::ExpectScreenshot {
            target, reference, ..
        } => {
            s.push_str("{\"op\":\"expect-screenshot\",\"target\":");
            q(target, s);
            s.push_str(",\"reference\":");
            q(reference, s);
        }
        Step::ExpectFocused { target, .. } => {
            s.push_str("{\"op\":\"expect-focused\",\"target\":");
            q(target, s);
        }
        Step::ExpectLayout {
            target,
            dimension,
            comparison,
            value,
            tolerance,
            ..
        } => {
            s.push_str("{\"op\":\"expect-layout\",\"target\":");
            q(target, s);
            s.push_str(&format!(
                ",\"dimension\":\"{dimension}\",\"comparison\":\"{}\",\"value\":",
                comparison_name(*comparison)
            ));
            match value {
                contract_syntax::LayoutOperand::Points(n) => s.push_str(&n.to_string()),
                contract_syntax::LayoutOperand::BoxMeasure { target, dimension } => {
                    s.push_str("{\"target\":");
                    q(target, s);
                    s.push_str(",\"dimension\":");
                    q(dimension, s);
                    s.push('}');
                }
            }
            if let Some(tolerance) = tolerance {
                s.push_str(&format!(",\"tolerance\":{tolerance}"));
            }
        }
        Step::ExpectText { target, value, .. } => {
            s.push_str("{\"op\":\"expect-text\",\"target\":");
            q(target, s);
            s.push_str(",\"value\":");
            q(value, s);
        }
        Step::ExpectWakelock { held, .. } => {
            s.push_str(&format!("{{\"op\":\"expect-wakelock\",\"held\":{held}"));
        }
        Step::ExpectDrag {
            target,
            kind,
            present,
            value,
            name,
            ..
        } => {
            s.push_str("{\"op\":\"expect-drag\",\"target\":");
            q(target, s);
            s.push_str(&format!(",\"present\":{present}"));
            for (key, field) in [("kind", kind), ("value", value), ("name", name)] {
                if let Some(text) = field {
                    s.push_str(&format!(",\"{key}\":"));
                    q(text, s);
                }
            }
        }
        Step::ExpectSound {
            src,
            present,
            at,
            gain,
            ends,
            playback_rate,
            pan,
            by,
            ..
        } => {
            s.push_str("{\"op\":\"expect-sound\",\"src\":");
            q(src, s);
            s.push_str(&format!(",\"present\":{present}"));
            for (key, n) in [
                ("at", at),
                ("gain", gain),
                ("ends", ends),
                ("playbackRate", playback_rate),
                ("pan", pan),
            ] {
                if let Some(n) = n {
                    s.push_str(&format!(",\"{key}\":{n}"));
                }
            }
            if let Some(by) = by {
                s.push_str(",\"by\":");
                q(by, s);
            }
        }
        Step::ExpectMediaSession { field, value, .. } => {
            s.push_str("{\"op\":\"expect-mediasession\",\"field\":");
            q(field, s);
            s.push_str(",\"value\":");
            match value {
                Some(v) => q(v, s),
                None => s.push_str("null"),
            }
        }
        Step::ExpectMediaSessionAction {
            action, present, ..
        } => {
            s.push_str("{\"op\":\"expect-mediasession\",\"action\":");
            q(action, s);
            s.push_str(&format!(",\"present\":{present}"));
        }
        // @ref LLP 1125 §4 — the driver's `prefer midi.*` and `tap midi`, and `state.midi.sent`.
        Step::Midi { step, .. } => midi_step(step, s),
        Step::ExpectState {
            name,
            comparison,
            value,
            ..
        } => {
            s.push_str("{\"op\":\"expect-state\",\"name\":");
            q(name, s);
            s.push_str(&format!(
                ",\"comparison\":\"{}\"",
                comparison_name(*comparison)
            ));
            match value {
                contract_syntax::StateOperand::Literal(value) => {
                    s.push_str(",\"value\":");
                    state_literal(value, s);
                }
                contract_syntax::StateOperand::State(path) => {
                    s.push_str(",\"state\":");
                    q(path, s);
                }
            }
        }
        Step::Perf { steps, .. } => {
            s.push_str("{\"op\":\"perf\",\"steps\":");
            steps_json(steps, s);
        }
        Step::ExpectPerf {
            subject,
            target,
            at_most,
            value,
            ..
        } => {
            s.push_str("{\"op\":\"expect-perf\",\"subject\":");
            q(subject, s);
            s.push_str(",\"target\":");
            q(target, s);
            s.push_str(&format!(
                ",\"compare\":\"{}\",\"value\":{value}",
                if *at_most { "<=" } else { "==" }
            ));
        }
        Step::ExpectNoFailures { .. } => s.push_str("{\"op\":\"expect-no-failures\""),
    }
    s.push_str(&format!(",\"line\":{line}}}"));
}

/// A MIDI step's JSON: `prefer` with its `midi` facts, `tap-midi`, or
/// `expect-midi`, the bytes as numbers.
fn midi_step(step: &contract_syntax::MidiStep, s: &mut String) {
    use contract_syntax::MidiStep;
    let bytes = |data: &[u8]| {
        let b: Vec<String> = data.iter().map(|b| b.to_string()).collect();
        format!("[{}]", b.join(","))
    };
    let names = |list: &[String], s: &mut String| {
        s.push('[');
        for (i, n) in list.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            q(n, s);
        }
        s.push(']');
    };
    match step {
        MidiStep::Prefer {
            state,
            inputs,
            outputs,
        } => {
            s.push_str("{\"op\":\"prefer-midi\"");
            if let Some(state) = state {
                s.push_str(",\"state\":");
                q(state, s);
            }
            if let Some(inputs) = inputs {
                s.push_str(",\"inputs\":");
                names(inputs, s);
            }
            if let Some(outputs) = outputs {
                s.push_str(",\"outputs\":");
                names(outputs, s);
            }
        }
        MidiStep::Tap { input, data } => {
            s.push_str("{\"op\":\"tap-midi\",\"input\":");
            q(input, s);
            s.push_str(&format!(",\"data\":{}", bytes(data)));
        }
        MidiStep::TapClock { input, bpm, ms } => {
            s.push_str("{\"op\":\"tap-midi\",\"input\":");
            q(input, s);
            s.push_str(&format!(",\"clock\":{{\"bpm\":{bpm},\"ms\":{ms}}}"));
        }
        MidiStep::Expect {
            output,
            data,
            at,
            present,
        } => {
            s.push_str("{\"op\":\"expect-midi\",\"output\":");
            q(output, s);
            s.push_str(&format!(",\"data\":{},\"present\":{present}", bytes(data)));
            if let Some(at) = at {
                s.push_str(&format!(",\"at\":{at}"));
            }
        }
    }
}

/// The test file's scripts and parsed property lines for the driver.
pub fn authored_json(tests: &[TestDecl], properties: &[contract_syntax::PropertyDecl]) -> String {
    use contract_syntax::{PropertyForm as F, PropertyKind as K, PropertyTest as T};
    let mut s = format!("{{\"tests\":{},\"properties\":[", tests_json(tests));
    for (i, p) in properties.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str("{\"name\":");
        q(&p.name, &mut s);
        s.push_str(",\"kind\":");
        q(
            match &p.kind {
                K::Always => "always",
                K::After(_) => "after",
                K::Clock => "clock",
                K::NeverRefuses => "never-refuses",
            },
            &mut s,
        );
        if let K::After(action) = &p.kind {
            s.push_str(",\"action\":");
            q(action, &mut s);
        }
        s.push_str(&format!(",\"line\":{},\"lines\":[", p.span.line));
        for (j, line) in p.lines.iter().enumerate() {
            if j > 0 {
                s.push(',');
            }
            s.push_str(&format!("{{\"line\":{},", line.span.line));
            match &line.form {
                F::Hold { name, test } => {
                    s.push_str("\"op\":\"expect-state\",\"name\":");
                    q(name, &mut s);
                    s.push_str(",\"test\":");
                    q(
                        match test {
                            T::Equal(_) => "equal",
                            T::Old => "old",
                            T::Add(_) => "add",
                            T::Subtract(_) => "subtract",
                            T::NotOld => "not-old",
                            T::In(_) => "in",
                            T::AtLeast(_) => "at-least",
                            T::AtMost(_) => "at-most",
                            T::Whole => "whole",
                            T::Stays(_) => "stays",
                        },
                        &mut s,
                    );
                    match test {
                        T::Equal(v) | T::Stays(v) => {
                            s.push_str(",\"value\":");
                            property_value(v, &mut s);
                        }
                        T::Add(n) | T::Subtract(n) | T::AtLeast(n) | T::AtMost(n) => {
                            s.push_str(&format!(",\"number\":{n}"))
                        }
                        T::In(values) => {
                            s.push_str(",\"values\":");
                            property_values(values, &mut s);
                        }
                        _ => {}
                    }
                }
                F::OnlyWrites(names) => {
                    s.push_str("\"op\":\"only-writes\",\"names\":[");
                    for (k, name) in names.iter().enumerate() {
                        if k > 0 {
                            s.push(',');
                        }
                        q(name, &mut s);
                    }
                    s.push(']');
                }
                F::Command { name, args } => {
                    s.push_str("\"op\":\"command\",\"name\":");
                    q(name, &mut s);
                    s.push_str(",\"args\":");
                    property_values(args, &mut s);
                }
            }
            s.push('}');
        }
        s.push_str("]}");
    }
    s.push_str("]}");
    s
}

fn property_values(values: &[contract_syntax::PropertyValue], s: &mut String) {
    s.push('[');
    for (i, v) in values.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        property_value(v, s);
    }
    s.push(']');
}
fn property_value(v: &contract_syntax::PropertyValue, s: &mut String) {
    use contract_syntax::PropertyValue as V;
    match v {
        V::Number(n) => s.push_str(&format!("{{\"kind\":\"number\",\"value\":{n}}}")),
        V::String(t) => {
            s.push_str("{\"kind\":\"string\",\"value\":");
            q(t, s);
            s.push('}');
        }
        V::Bool(b) => s.push_str(&format!("{{\"kind\":\"bool\",\"value\":{b}}}")),
        V::None => s.push_str("{\"kind\":\"none\"}"),
        V::Some => s.push_str("{\"kind\":\"some\"}"),
        V::EmptyList => s.push_str("{\"kind\":\"empty-list\"}"),
    }
}

/// A comparison as the driver's script names it.
fn comparison_name(comparison: contract_syntax::StateComparison) -> &'static str {
    match comparison {
        contract_syntax::StateComparison::Equal => "equal",
        contract_syntax::StateComparison::NotEqual => "not-equal",
        contract_syntax::StateComparison::AtLeast => "at-least",
        contract_syntax::StateComparison::AtMost => "at-most",
        contract_syntax::StateComparison::In => "in",
    }
}

fn state_literal(value: &Expr, s: &mut String) {
    match value {
        Expr::Number(n, _) => s.push_str(&format!("{n}")),
        Expr::Str(t, _) => q(t, s),
        Expr::Bool(b, _) => s.push_str(&format!("{b}")),
        Expr::Some(inner, _) => state_literal(inner, s),
        Expr::List(items, _) => {
            s.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    s.push(',');
                }
                state_literal(item, s);
            }
            s.push(']');
        }
        Expr::Call(_, fields, _) => {
            s.push('{');
            for (i, field) in fields.iter().enumerate() {
                let Expr::NamedArg(name, value, _) = field else {
                    unreachable!("state literal fields are named")
                };
                if i > 0 {
                    s.push(',');
                }
                q(name, s);
                s.push(':');
                state_literal(value, s);
            }
            s.push('}');
        }
        Expr::None(_) => s.push_str("null"),
        _ => unreachable!("state expectations contain only literals"),
    }
}

#[cfg(test)]
#[test]
fn an_ios_background_interval_is_a_real_time_test_step() {
    let parsed = crate::tests("test \"resume\" on ios\n  background 1250\n").unwrap();
    let json: serde_json::Value = serde_json::from_str(&crate::tests_json(&parsed)).unwrap();
    assert_eq!(
        json[0]["steps"][0],
        serde_json::json!({"op":"background", "ms":1250, "line":2})
    );
    for ms in ["", "-1", "0.5", "2147483648"] {
        assert!(crate::tests(&format!("test \"t\"\n  background {ms}\n")).is_err());
    }
    assert!(crate::tests("test \"t\"\n  perf\n    background 100\n").is_err());
}
