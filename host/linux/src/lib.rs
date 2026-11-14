//! The Linux host.
//!
//! @ref LLP 1015 (Linux host v1)
//!
//! The painted host (`exact-painted`) lays the kernel tree out and paints
//! it; this crate shows it on Linux. The pixels go to DRM/KMS dumb buffers
//! with evdev input, or into a buffer with no display at all, which is how
//! the host is tested: the agent API (LLP 1012) over stdio, a screenshot, a
//! smoke run — on a fleet Linux box with no GPU, or on macOS in the
//! seconds-loop.
//!
//! - [`display`] — KMS dumb buffers and the loop that presents into them.
//! - [`input`] — evdev: keyboards, pointers, touch and gamepads.
//! - [`vnc`] — the screen over RFB, with a client's pointer and keys as input.
//! - [`run`] and its kin — the entry: the environment, headless or display.

#![deny(unsafe_code)]
#![deny(missing_docs)]

#[cfg(target_os = "linux")]
pub mod display;
#[cfg(all(test, unix, not(target_os = "linux")))]
#[path = "display/flip.rs"]
mod display_flip;
#[cfg(target_os = "linux")]
pub mod input;
#[cfg(target_os = "linux")]
pub mod vnc;
#[cfg(all(test, unix, not(target_os = "linux")))]
#[path = "vnc/auth.rs"]
mod vnc_auth;

use exact_painted::app::{print_baked_receipt, row_reuse_from_env, Config};
use exact_painted::content_region::ContentRegionRegistration;
use exact_painted::Composed;
use exact_runner::DataSource;
use std::time::Instant;

/// Run a Contract app without application data, sharing its compiled host.
pub fn run_empty(baked: &[u8], compat: &str) -> i32 {
    run::<()>(baked, compat)
}

/// Run the app: the process's exit code. `compat` is the binary's
/// `compat.json` (LLP 1030 D3a), which the `delivery` resource answers from.
pub fn run<D: DataSource + Default>(baked: &[u8], compat: &str) -> i32 {
    run_composed::<D>(baked, compat, Composed::default())
}

/// [`run`] with the app's hatches (LLP 1075.003.000.001 §5): `H` is the one
/// type its `modules/linux` names, and `words` the hatch words `app.json`
/// gives this platform. Nothing of `H` is made before the first frame is
/// presented.
pub fn run_with_hatches<D: DataSource + Default, H: exact_painted::Hatches>(
    baked: &[u8],
    compat: &str,
    words: &'static [&'static str],
) -> i32 {
    let hatches = Some(exact_painted::hatches::install::<H>(words));
    run_composed::<D>(
        baked,
        compat,
        Composed {
            hatches,
            ..Default::default()
        },
    )
}

/// [`run`] with what the app's build composed: its hatches, if it has
/// any, and its packages' Linux implementations (`exact_painted::package`).
pub fn run_composed<D: DataSource + Default>(
    baked: &[u8],
    compat: &str,
    composed: Composed,
) -> i32 {
    run_composed_with::<D>(baked, compat, None, composed)
}

/// Launch an explicitly registered consumer using the ordinary environment,
/// receipt and store preflight. No generic worker or authoring schema is added.
pub fn run_with_content_region<D: DataSource + Default>(
    baked: &[u8],
    compat: &str,
    region: ContentRegionRegistration,
) -> i32 {
    run_composed_with::<D>(baked, compat, Some(region), Composed::default())
}

fn run_composed_with<D: DataSource + Default>(
    baked: &[u8],
    compat: &str,
    region: Option<ContentRegionRegistration>,
    composed: Composed,
) -> i32 {
    row_reuse_from_env(false);
    if print_baked_receipt(compat) {
        return 0;
    }
    if let Err(why) = exact_runner::delivery::refuse_analysis(compat) {
        eprintln!("exact: {why}");
        return 1;
    }
    if exact_runner::Delivery::default().with_compat(compat).store != '0' {
        eprintln!(
            "exact: the baked store level requires the delivery adapter; regenerate the app entry"
        );
        return 1;
    }
    let started = Instant::now();
    let mut config = Config::from_env(baked, compat);
    config.content_region = region;
    config.hatches = composed.hatches;
    config.packages = composed.packages;
    run_config::<D>(&mut config, started)
}

/// Run a configured app, optionally composed with a delivery adapter.
pub fn run_config<D: DataSource + Default>(config: &mut Config, started: Instant) -> i32 {
    if let Err(why) = exact_runner::delivery::refuse_analysis(&config.compat) {
        eprintln!("exact: {why}");
        return 1;
    }
    exact_painted::app::show_neutral_look();
    if config.headless() {
        return exact_painted::app::run_headless::<D>(config, started, None);
    }
    #[cfg(target_os = "linux")]
    {
        display::run::<D>(config, started)
    }
    #[cfg(not(target_os = "linux"))]
    {
        1
    }
}

#[cfg(test)]
mod tests {
    use exact_runner::{DataError, DataSource, Value};

    #[test]
    fn an_analysis_core_entry_refuses_before_constructing_app_data() {
        struct MustNotBoot;
        impl Default for MustNotBoot {
            fn default() -> Self {
                panic!("analysis must never construct the app")
            }
        }
        impl DataSource for MustNotBoot {
            fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
                unreachable!()
            }
        }
        let compat = r#"{"inputs":{"store":{"L":"0"}},"embedded":{"analysis":true,"seq":null}}"#;
        assert_eq!(super::run::<MustNotBoot>(b"EXPL", compat), 1);
    }

    #[test]
    fn a_core_only_entry_refuses_an_update_capable_compatibility_record() {
        assert_eq!(
            super::run::<()>(b"not read", r#"{"inputs":{"store":{"L":"A"}}}"#),
            1
        );
    }
}

#[cfg(test)]
mod executor_tests;
