//! The Linux host's integration tests that leave the font environment alone:
//! one binary. `tests/pinned/` holds the ones that pin the fixture font, so
//! neither changes the other's process environment.

mod accent;
mod animation;
mod arrange;
mod box_filter;
mod colors;
mod focus;
mod font_stack;
mod forma;
mod grouped_rows;
mod height;
mod height_binding;
mod hit_slop;
mod holds;
mod image;
mod list_rows;
mod live;
mod media_session;
mod per_axis_scale;
mod presence;
mod svg;
mod timeline;
mod top_layer;
mod transform_binding;
mod transform_contact;
mod viewport;

#[test]
fn a_cold_share_is_claimed_in_either_drive_mode() {
    const CASE: &str = "EXACT_TEST_COLD_SHARE";
    const TEST: &str = "a_cold_share_is_claimed_in_either_drive_mode";
    if let Ok(mode) = std::env::var(CASE) {
        let share = exact_painted::picker::cold_share(1.);
        if mode.is_empty() {
            assert!(share.is_none());
        } else {
            assert_eq!(share.unwrap().0, "/share?text=hello");
            assert!(exact_painted::picker::cold_share(1.).is_none());
        }
        return;
    }
    let request = serde_json::json!({
        "target": {"action":"/share","params":{"text":"text"}},
        "share": {"text":"hello"}
    });
    for mode in ["", "1", "live"] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST])
            .env(CASE, mode)
            .env("EXACT_AGENT", mode)
            .env("EXACT_AGENT_SHARE", request.to_string())
            .env_remove("EXACT_SHARE")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{mode}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
}
