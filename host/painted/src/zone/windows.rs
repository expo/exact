//! Windows keeps the preferred UI language apart from the regional format.
use windows_sys::Win32::Globalization::{
    GetLocaleInfoEx, GetUserDefaultLocaleName, GetUserPreferredUILanguages, LOCALE_NOUSEROVERRIDE,
    LOCALE_SSHORTTIME, MUI_LANGUAGE_NAME,
};
use windows_sys::Win32::System::SystemServices::LOCALE_NAME_MAX_LENGTH;

pub(super) fn locales() -> (String, String) {
    let regional = regional_locale().unwrap_or_else(|| "en-US".into());
    let locale = preferred_language().unwrap_or_else(|| regional.clone());
    // A null locale name is LOCALE_NAME_USER_DEFAULT. Read the override and
    // the locale's default separately, so only a forced cycle enters the tag.
    // https://learn.microsoft.com/windows/win32/api/winnls/nf-winnls-getlocaleinfoex
    let pattern = |flags| {
        wide_string(|buffer, size| unsafe {
            GetLocaleInfoEx(std::ptr::null(), LOCALE_SSHORTTIME | flags, buffer, size)
        })
        .unwrap_or_default()
    };
    let format = format_locale(&regional, &pattern(0), &pattern(LOCALE_NOUSEROVERRIDE));
    (locale, format)
}

fn regional_locale() -> Option<String> {
    let mut buffer = [0; LOCALE_NAME_MAX_LENGTH as usize];
    let written = unsafe { GetUserDefaultLocaleName(buffer.as_mut_ptr(), buffer.len() as i32) };
    if written <= 1 {
        return None;
    }
    let name = String::from_utf16(&buffer[..written as usize - 1]).ok()?;
    // An alternate sort follows `_` (`es-ES_tradnl`, `de-DE_phoneb`), which no BCP 47 tag has.
    Some(name.split('_').next().unwrap_or_default().to_owned())
}

fn wide_string(mut read: impl FnMut(*mut u16, i32) -> i32) -> Option<String> {
    let size = read(std::ptr::null_mut(), 0);
    if size <= 0 {
        return None;
    }
    let mut buffer = vec![0; size as usize];
    let written = read(buffer.as_mut_ptr(), size);
    if written <= 1 || written > size {
        return None;
    }
    String::from_utf16(&buffer[..written as usize - 1]).ok()
}

fn preferred_language() -> Option<String> {
    let (mut count, mut size) = (0, 0);
    // The API returns a double-NUL-terminated list in preference order.
    // https://learn.microsoft.com/windows/win32/api/winnls/nf-winnls-getuserpreferreduilanguages
    unsafe {
        if GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &mut count,
            std::ptr::null_mut(),
            &mut size,
        ) == 0
            || size == 0
        {
            return None;
        }
        let mut buffer = vec![0; size as usize];
        if GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &mut count,
            buffer.as_mut_ptr(),
            &mut size,
        ) == 0
        {
            return None;
        }
        let end = buffer.iter().position(|&unit| unit == 0)?;
        (end > 0)
            .then(|| String::from_utf16(&buffer[..end]).ok())
            .flatten()
    }
}

fn hour_cycle(pattern: &str) -> Option<&'static str> {
    let mut quoted = false;
    for ch in pattern.chars() {
        match ch {
            '\'' => quoted = !quoted,
            'h' if !quoted => return Some("h12"),
            'H' if !quoted => return Some("h23"),
            _ => {}
        }
    }
    None
}

fn format_locale(locale: &str, pattern: &str, regional_pattern: &str) -> String {
    let (Some(cycle), Some(regional)) = (hour_cycle(pattern), hour_cycle(regional_pattern)) else {
        return locale.into();
    };
    if cycle == regional {
        return locale.into();
    }
    // Windows supplemental locales can end in a private-use subtag. Unicode
    // keywords precede that subtag, and the runner admits at most 64 bytes.
    let at = locale.find("-x-").unwrap_or(locale.len());
    let tag = format!("{}-u-hc-{cycle}{}", &locale[..at], &locale[at..]);
    if tag.len() <= 64 {
        tag
    } else {
        locale.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_changed_hour_cycle_enters_the_regional_tag() {
        for (locale, chosen, regional, expected) in [
            ("en-US", "HH:mm", "h:mm tt", "en-US-u-hc-h23"),
            ("en-GB", "h:mm tt", "HH:mm", "en-GB-u-hc-h12"),
            ("en-US", "hh.mm tt", "h:mm tt", "en-US"),
            ("fr-CA", "HH 'h' mm", "HH:mm", "fr-CA"),
            ("en-US", "'H' h:mm tt", "h:mm tt", "en-US"),
            ("en-US", "'o''clock h' HH:mm", "h:mm tt", "en-US-u-hc-h23"),
            ("en-US", "", "h:mm tt", "en-US"),
            ("en-US", "HH:mm", "", "en-US"),
            ("en-US", "'hours' mm", "h:mm tt", "en-US"),
            (
                "en-US-x-custom",
                "HH:mm",
                "h:mm tt",
                "en-US-u-hc-h23-x-custom",
            ),
        ] {
            assert_eq!(format_locale(locale, chosen, regional), expected);
        }
        let long = format!("en-US-x-{}", "a".repeat(50));
        assert_eq!(format_locale(&long, "HH:mm", "h:mm tt"), long);
    }

    #[test]
    fn windows_reports_valid_language_and_regional_tags() {
        let (locale, format_locale) = locales();
        assert!(exact_runner::time::Place {
            locale,
            format_locale,
            ..Default::default()
        }
        .validate()
        .is_ok());
        assert!(preferred_language().is_some());
        assert!(regional_locale().is_some());
    }
}
