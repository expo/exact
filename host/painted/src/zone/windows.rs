//! Windows locale preferences and time-zone rules.
use windows_sys::Win32::Globalization::{
    GetLocaleInfoEx, GetUserDefaultLocaleName, GetUserPreferredUILanguages, LOCALE_NOUSEROVERRIDE,
    LOCALE_SSHORTTIME, MUI_LANGUAGE_NAME,
};
use windows_sys::Win32::System::SystemServices::LOCALE_NAME_MAX_LENGTH;

/// The live offset in minutes east of UTC, including the user's daylight-saving setting.
pub fn local_offset_minutes() -> f64 {
    use windows_sys::Win32::System::{
        SystemServices::{TIME_ZONE_ID_DAYLIGHT, TIME_ZONE_ID_STANDARD, TIME_ZONE_ID_UNKNOWN},
        Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION},
    };
    let mut zone = TIME_ZONE_INFORMATION::default();
    let adjustment = match unsafe { GetTimeZoneInformation(&mut zone) } {
        TIME_ZONE_ID_DAYLIGHT => zone.DaylightBias,
        TIME_ZONE_ID_STANDARD => zone.StandardBias,
        TIME_ZONE_ID_UNKNOWN => 0,
        _ => return 0.0,
    };
    -f64::from(zone.Bias) - f64::from(adjustment)
}

/// ICU also recognizes custom offsets such as GMT+04:00. Only system IDs
/// can name a zone in the launch place. Keep the caller's spelling of aliases.
pub(super) fn system_zone_id(name: &str) -> Option<Vec<u16>> {
    use windows_sys::Win32::Globalization::{
        ucal_getCanonicalTimeZoneID, U_BUFFER_OVERFLOW_ERROR, U_ZERO_ERROR,
    };
    // ICU treats an embedded NUL as the end of the ID even with an explicit length.
    if name.as_bytes().contains(&0) {
        return None;
    }
    crate::com::ensure_mta().ok()?;
    let id: Vec<_> = name.encode_utf16().collect();
    let mut canonical = [0; 64];
    let mut system = 0;
    let mut status = U_ZERO_ERROR;
    unsafe {
        ucal_getCanonicalTimeZoneID(
            id.as_ptr(),
            id.len().try_into().ok()?,
            canonical.as_mut_ptr(),
            canonical.len() as i32,
            &mut system,
            &mut status,
        );
    }
    (system != 0 && (status <= U_ZERO_ERROR || status == U_BUFFER_OVERFLOW_ERROR)).then_some(id)
}

struct Calendar {
    zone: String,
    raw: std::ptr::NonNull<*mut std::ffi::c_void>,
}

impl Calendar {
    fn open(zone: &str) -> Option<Self> {
        use windows_sys::Win32::Globalization::{ucal_open, UCAL_GREGORIAN, U_ZERO_ERROR};
        let id = system_zone_id(zone)?;
        let mut status = U_ZERO_ERROR;
        let raw = unsafe {
            ucal_open(
                id.as_ptr(),
                id.len() as i32,
                c"en_US_POSIX".as_ptr().cast(),
                UCAL_GREGORIAN,
                &mut status,
            )
        };
        let calendar = Self {
            zone: zone.to_owned(),
            raw: std::ptr::NonNull::new(raw)?,
        };
        (status <= U_ZERO_ERROR).then_some(calendar)
    }

    fn offset_minutes(&mut self, epoch_ms: f64) -> f64 {
        use windows_sys::Win32::Globalization::{
            ucal_get, ucal_setMillis, UCAL_DST_OFFSET, UCAL_ZONE_OFFSET, U_ZERO_ERROR,
        };
        let mut status = U_ZERO_ERROR;
        let cal = self.raw.as_ptr();
        let (base, dst) = unsafe {
            ucal_setMillis(cal, epoch_ms, &mut status);
            (
                ucal_get(cal.cast(), UCAL_ZONE_OFFSET, &mut status),
                ucal_get(cal.cast(), UCAL_DST_OFFSET, &mut status),
            )
        };
        if status > U_ZERO_ERROR {
            return 0.0;
        }
        (f64::from(base) + f64::from(dst)) / 60_000.0
    }
}

impl Drop for Calendar {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Globalization::ucal_close(self.raw.as_ptr()) };
    }
}

pub(crate) fn offset_minutes_at(zone: &str, epoch_ms: f64) -> f64 {
    if matches!(zone, "UTC" | "Etc/UTC") {
        return 0.0;
    }
    // A frame can format many instants in the same zone. ICU calendars are mutable.
    thread_local! {
        static LAST: std::cell::RefCell<Option<Calendar>> = const { std::cell::RefCell::new(None) };
    }
    LAST.with_borrow_mut(|last| {
        if last.as_ref().is_none_or(|calendar| calendar.zone != zone) {
            *last = Calendar::open(zone);
        }
        last.as_mut()
            .map_or(0.0, |calendar| calendar.offset_minutes(epoch_ms))
    })
}

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
    fn system_zones_keep_aliases_and_follow_transitions_at_the_given_instant() {
        use crate::zone::zone_name;
        assert_eq!(zone_name(":US/Eastern").as_deref(), Some("US/Eastern"));
        for invalid in ["Etc/Unknown", "GMT+04:00", "Invalid/Zone", "UTC\0extra"] {
            assert_eq!(zone_name(invalid), None, "{invalid}");
            assert_eq!(offset_minutes_at(invalid, 0.0), 0.0, "{invalid}");
        }
        for (zone, epoch_ms, minutes) in [
            ("US/Eastern", 1_710_053_999_999., -300.),
            ("US/Eastern", 1_710_054_000_000., -240.),
            ("US/Eastern", 1_730_613_599_999., -240.),
            ("US/Eastern", 1_730_613_600_000., -300.),
            ("Asia/Kathmandu", 1_710_054_000_000., 345.),
            ("Australia/Lord_Howe", 1_712_415_599_999., 660.),
            ("Australia/Lord_Howe", 1_712_415_600_000., 630.),
            ("America/Toronto", -15_854_400_000., -240.),
        ] {
            assert_eq!(
                offset_minutes_at(zone, epoch_ms),
                minutes,
                "{zone} at {epoch_ms}"
            );
        }
    }

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
