//! The modifier tokens of an authored `aria-keyshortcuts` chord (LLP 1021 §5.3).

/// Whether any chord in the list needs the reader's primary modifier.
pub fn portable(list: &str) -> bool {
    list.split(|ch: char| ch.is_whitespace() || ch == '\u{feff}')
        .any(|chord| chord.split('+').any(|part| part == "Mod"))
}

/// Resolve one chord for a host, preserving its key and modifier order.
/// A portable chord containing an explicit primary modifier is ambiguous.
pub fn resolve<'a>(chord: &'a str, primary: &str) -> Option<std::borrow::Cow<'a, str>> {
    use std::borrow::Cow;
    let Some((held, key)) = chord.rsplit_once('+') else {
        return Some(Cow::Borrowed(chord));
    };
    if !held.split('+').any(|m| m == "Mod") {
        return Some(Cow::Borrowed(chord));
    }
    if chord.split('+').any(|m| matches!(m, "Meta" | "Control")) {
        return None;
    }
    let mut resolved = String::new();
    for modifier in held.split('+') {
        resolved.push_str(if modifier == "Mod" { primary } else { modifier });
        resolved.push('+');
    }
    resolved.push_str(key);
    Some(Cow::Owned(resolved))
}
