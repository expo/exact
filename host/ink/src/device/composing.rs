//! Whether a RichEdit field is composing (`d3d11::fields`). The message hook
//! dispatches each IME message to the focused field while it holds every
//! field; a synchronous IME message can arrive inside that dispatch, where
//! the hook cannot take them again and leaves the message to Windows. An end
//! of composition that arrives so is kept and applied when the outer call
//! has dispatched, so no composition outlives its end.

/// `WM_IME_STARTCOMPOSITION`
/// (<https://learn.microsoft.com/windows/win32/intl/wm-ime-startcomposition>).
pub const START: u32 = 0x010D;
/// `WM_IME_ENDCOMPOSITION`
/// (<https://learn.microsoft.com/windows/win32/intl/wm-ime-endcomposition>).
pub const END: u32 = 0x010E;

/// Whether the field composes once `message` has been dispatched to it:
/// `accepted` when the control took the message without error, `ended` when
/// an end arrived while the hook held the fields. A start begins a
/// composition only if the control accepted it; an end finishes it whatever
/// the control answered.
pub fn after(composing: bool, message: u32, accepted: bool, ended: bool) -> bool {
    match message {
        _ if ended => false,
        END => false,
        START => accepted,
        _ => composing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEYDOWN: u32 = 0x0100;
    const COMPOSITION: u32 = 0x010F;

    #[test]
    fn no_composition_outlives_its_end_or_a_start_the_control_refused() {
        assert!(after(false, START, true, false));
        assert!(!after(false, START, false, false), "a start that failed");
        assert!(after(true, COMPOSITION, true, false));
        assert!(
            after(true, COMPOSITION, false, false),
            "a failed update ends nothing"
        );
        assert!(!after(true, END, true, false));
        assert!(!after(true, END, false, false), "an end that failed");
        assert!(
            !after(true, COMPOSITION, true, true),
            "an end that arrived nested"
        );
        assert!(
            !after(false, START, true, true),
            "a composition that ended as it began"
        );
        assert!(!after(false, KEYDOWN, true, false));
    }
}
