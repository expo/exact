//! A widget entry without a presenter (LLP 1126.003 D8): the entry's own
//! plan booted at one size, laid out by the kernel with this host's text,
//! and walked into the tree Android's `RemoteViews` draw (D2, `tree.rs`);
//! and the entry's refresh, run as Apple's extension runs it (LLP 1126 D3).
//!
//! The render endows no I/O, reads no store or secret, opens no delivery
//! and installs no presenter, painter, executor or hatch. Only the refresh
//! reads: the store as the app left it, and its requests through this
//! host's executor.

mod jni;
#[cfg(test)]
mod tests;
mod tree;

use crate::text::{Measurer, Shared, TextEngine};
use exact_kernel::{Kernel, Offer};
use exact_plan::Plan;
use exact_runner::{DataSource, Runner};
use std::path::Path;
use std::time::{Duration, Instant};

/// One widget entry's input: its plan (the bake's `plans/<kind>.plan`), its
/// kind, and the props and configuration a host supplies. Either left out
/// keeps what the bake compiled in: the row's preview, the defaults.
#[derive(Clone, Copy)]
pub struct Entry<'a> {
    /// The entry's plan.
    pub plan: &'a [u8],
    /// The row's `kind`.
    pub kind: &'a str,
    /// The props, JSON of the row's props shape.
    pub props: Option<&'a [u8]>,
    /// A placement's chosen configuration, JSON.
    pub configuration: Option<&'a [u8]>,
}

/// The longest a refresh may take, boot included: WidgetKit's, which
/// Android takes too (LLP 1126.003 D5).
pub const REFRESH_BUDGET: Duration = Duration::from_millis(900);

/// The root font size a font scale of 1 gives, as the reader's (`presenter::device`).
const ROOT_FONT_SIZE: f64 = 16.0;

/// An entry booted and laid out at one size.
pub struct Session<D: DataSource> {
    runner: Runner<D>,
    text: Shared,
}

impl<D: DataSource> Session<D> {
    /// Boot `entry` at `size` (points, Android's dp) at `now` (ms since
    /// the epoch) under `font_scale`, with `assets` the app's asset
    /// directory (its declared faces and pictures), and lay it out. The
    /// viewer's locale and zone are the process's (`zone::launch_place`).
    pub fn boot(
        entry: Entry<'_>,
        data: D,
        assets: &Path,
        size: (f32, f32),
        now: f64,
        font_scale: f32,
    ) -> Result<Self, String> {
        let plan = Plan::decode_snapshot(entry.plan, entry.kind)
            .map_err(|e| format!("widget {}: plan: {e:?}", entry.kind))?;
        let carried = exact_runner::entry::slots(&plan, entry.props, entry.configuration)?;
        link_grammars();
        let text = TextEngine::shared_for_plan(&plan, assets);
        let kernel = Kernel::new(Box::new(Measurer(text.clone())));
        let launch = exact_runner::time::Launch {
            time: Some(exact_runner::time::WallTime {
                epoch_at_zero: now,
                utc_offset: crate::zone::local_offset_minutes(),
            }),
            place: crate::zone::launch_place(|key| std::env::var(key).ok()).ok(),
            ..exact_runner::time::Launch::NONE
        };
        let mut runner = exact_runner::time::launching_with(launch, || {
            Runner::boot_carrying(
                plan,
                data,
                kernel,
                &carried,
                exact_runner::Viewport::sized(size.0 as f64, size.1 as f64),
                "/",
            )
        })
        .map_err(|e| format!("widget {}: boot: {e:?}", entry.kind))?;
        if font_scale.is_finite() && font_scale > 0.0 && font_scale != 1.0 {
            runner
                .set_root_font_size(ROOT_FONT_SIZE * f64::from(font_scale))
                .map_err(|e| format!("widget {}: font scale: {e:?}", entry.kind))?;
        }
        for root in runner.roots() {
            runner
                .kernel_mut()
                .compute_layout(root, Offer::definite(size.0, size.1))
                .map_err(|e| format!("widget {}: layout: {e:?}", entry.kind))?;
        }
        Ok(Session { runner, text })
    }

    /// The laid-out entry as D2's snapshot tree, JSON.
    pub fn tree(&self) -> serde_json::Value {
        tree::walk(self.runner.kernel(), &self.runner.roots(), &self.text)
    }
}

/// Render `entry` at each of `sizes` at `now`: `{"sizes": [{"width",
/// "height", "root"}]}`.
pub fn render<D: DataSource>(
    entry: Entry<'_>,
    data: impl Fn() -> D,
    assets: &Path,
    sizes: &[(f32, f32)],
    now: f64,
    font_scale: f32,
) -> Result<String, String> {
    let mut out = Vec::with_capacity(sizes.len());
    for &(width, height) in sizes {
        let session = Session::boot(entry, data(), assets, (width, height), now, font_scale)?;
        out.push(serde_json::json!({ "width": width, "height": height, "root": session.tree() }));
    }
    Ok(serde_json::json!({ "sizes": out }).to_string())
}

/// A failure as the JSON a widget call answers: `{"error": "…"}`.
pub fn error_json(message: &str) -> String {
    serde_json::json!({ "error": message }).to_string()
}

/// Bytes aligned as Hermes bytecode must be (`widgets_jni!`'s `app.hbc`).
#[repr(C, align(16))]
pub struct Aligned<B: ?Sized>(pub B);

/// The entry's TypeScript module, as its bake paired it with the entry's
/// plan (`app.module.json`'s receipt and `app.hbc`).
#[derive(Clone, Copy)]
pub struct Module<'a> {
    /// `app.module.json`.
    pub receipt: &'a str,
    /// `app.hbc`.
    pub bytecode: &'a [u8],
}

/// Run the entry's refresh at `now` (ms since the epoch) within `deadline`:
/// the timeline its refresh function returns, JSON (`[{date, props}]`).
pub fn timeline<D: DataSource>(
    entry: Entry<'_>,
    data: D,
    module: Option<Module<'_>>,
    now: f64,
    deadline: Instant,
) -> Result<String, String> {
    if Instant::now() >= deadline {
        return Err("widget refresh timed out".into());
    }
    let plan = Plan::decode_snapshot(entry.plan, entry.kind)
        .map_err(|e| format!("widget {}: plan: {e:?}", entry.kind))?;
    link_grammars();
    let kernel = Kernel::new(Box::new(exact_kernel::MonospaceMeasurer::default()));
    let mut runner = Runner::boot(
        plan,
        data,
        kernel,
        // The refresh lays nothing out; a viewport must still be valid.
        exact_runner::Viewport::sized(1.0, 1.0),
        "/",
    )
    .map_err(|e| format!("widget {}: boot: {e:?}", entry.kind))?;
    let encoded = runner.plan().encode();
    let kind = runner.plan().snapshot_entry.clone();
    let data = runner.data();
    data.prepare_snapshot(
        module.map(|m| exact_runner::SnapshotModule {
            receipt: m.receipt,
            bytecode: m.bytecode,
            plan: &encoded,
            kind: &kind,
        }),
        deadline,
    )
    .map_err(|e| format!("snapshot module: {e:?}"))?;
    let app = data.app_id().to_owned();
    let grants = data.grants().to_owned();
    let roots = crate::picker::app_dirs(&app)
        .map_err(|e| format!("snapshot storage: {e:?}"))?
        .map(|(roots, _)| roots);
    if let Some([data_root, cache, temporary]) = roots.clone() {
        data.configure_storage(data_root, cache, temporary)
            .map_err(|e| format!("snapshot storage: {e:?}"))?;
    }
    // The module may run past the deadline in one step: its interrupt ends it.
    let interrupt = data.interrupt();
    let (cancel, wait) = std::sync::mpsc::channel::<()>();
    let watchdog = interrupt.map(|interrupt| {
        std::thread::spawn(move || {
            if wait
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .is_err()
            {
                interrupt.trigger();
            }
        })
    });
    // The store as the app left it, read only: a refresh writes nothing
    // (LLP 1126 D3, stage 1).
    let (snapshot, _) = crate::host::system::store_read(&app, false);
    let mut executor: Option<crate::executor::Executor> = None;
    let result = runner.snapshot_timeline(now, snapshot, deadline, |mut request, work| {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("widget refresh timed out".into());
        }
        let executor = executor.get_or_insert_with(|| {
            let executor = crate::executor::Executor::start(&grants);
            if let Some(roots) = roots.clone() {
                executor.set_app_roots(roots);
            }
            executor
        });
        if request.continuation.is_none() {
            let ms = remaining.as_millis().clamp(1, u32::MAX as u128) as u32;
            request.timeout_ms = Some(request.timeout_ms.map_or(ms, |own| own.min(ms)));
        }
        let class = if work.is_none() && request.is_plain_http() {
            exact_runner::RequestClass::Read
        } else {
            exact_runner::RequestClass::Ordered
        };
        executor
            .run(
                exact_runner::RequestOut {
                    completion: None,
                    class,
                    ticket: 1,
                    target: "widget refresh".into(),
                    request,
                    forced: false,
                },
                work,
            )
            .map_err(str::to_owned)?;
        loop {
            executor.begin_pump();
            if let Some((_, outcome, _)) = executor.drain().into_iter().next() {
                return Ok(outcome);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                executor.forget(|_| false);
                return Err("widget refresh timed out".into());
            }
            wait_readable(executor, remaining);
        }
    });
    drop(executor);
    let _ = cancel.send(());
    if let Some(watchdog) = watchdog {
        let _ = watchdog.join();
    }
    result
}

/// Wait until the executor's wake is readable or `limit` passes.
#[cfg(unix)]
fn wait_readable(executor: &crate::executor::Executor, limit: Duration) {
    let mut fd = libc::pollfd {
        fd: executor.fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    let ms = limit.as_millis().clamp(1, i32::MAX as u128) as i32;
    // SAFETY: one valid pollfd, for the call's duration.
    #[allow(unsafe_code)]
    unsafe {
        libc::poll(&mut fd, 1, ms);
    }
}

#[cfg(not(unix))]
fn wait_readable(_: &crate::executor::Executor, limit: Duration) {
    std::thread::sleep(limit.min(Duration::from_millis(2)));
}

/// What every boot of this host links (`Host::boot_decoded`): the grammars
/// a style row may need, and the colours this host draws (sRGB).
fn link_grammars() {
    exact_kernel::style::wide::set_available(exact_color::Wide::in_srgb);
    exact_kernel::style::link_backdrop_filter();
    exact_kernel::style::link_segments();
    exact_kernel::style::link_wide_colors();
    exact_kernel::timeline::link();
}
