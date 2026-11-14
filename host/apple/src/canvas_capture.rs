//! A canvas capture on Apple (LLP 1056 D9, as amended for expo/exact#188;
//! `exact_runner::canvas_capture`): the runner names the canvas, the
//! presenter encodes the bitmap it keeps as a PNG (`Canvas2DCapture.swift`)
//! and hands it back as surface work of kind 8, and this writes it beside its
//! target and renames it over it, under the source's `fs.write`, as
//! `exactBodyTo` writes a reply.

use super::Host;
use exact_runner::canvas_capture::{refused, CanvasCapture};
use exact_runner::{DataSource, Outcome, RequestOut};

/// A capture the presenter was handed: the canvas and the request's grant
/// scope, or the refusal it hands straight back.
pub(crate) type Pending = Result<PendingCapture, Outcome>;

pub(crate) struct PendingCapture {
    canvas: CanvasCapture,
    scope: Option<String>,
    copied: bool,
}

impl<D: DataSource> Host<D> {
    /// Surface work for the presenter: a canvas capture as its own op, any
    /// other as `Batch::surface_work`.
    pub fn surface_work(
        &mut self,
        presenter: &mut crate::batch::Batch,
        r: &RequestOut,
        refusal: Option<&str>,
    ) {
        if r.request.canvas_capture_pending() {
            self.canvas_capture_waiting
                .push((r.clone(), refusal.map(str::to_owned)));
            return;
        }
        let Some(found) = r.request.take_canvas_capture() else {
            return presenter.surface_work(r, refusal);
        };
        // The runner's refusal first: it checks the path itself.
        let pending = found.and_then(|c| match refusal {
            Some(why) => Err(refused("NotAllowedError", why)),
            None => Ok(PendingCapture {
                canvas: c,
                scope: r.request.grants.clone(),
                copied: false,
            }),
        });
        presenter.canvas_capture(r.ticket, pending.as_ref().ok().map(|p| &p.canvas));
        self.canvas_captures.insert(r.ticket, pending);
    }

    pub(crate) fn drain_canvas_captures(&mut self, batch: &mut crate::batch::Batch) {
        for (r, refusal) in std::mem::take(&mut self.canvas_capture_waiting) {
            if self.runner.holds(r.ticket) {
                self.surface_work(batch, &r, refusal.as_deref());
            }
        }
    }

    pub(crate) fn forget_canvas_captures(&mut self) {
        self.canvas_captures
            .retain(|ticket, _| self.runner.holds(*ticket));
    }

    /// Called on the runtime owner immediately after it acquires the pixel copy.
    pub fn canvas_snapshotted(&mut self, ticket: u64) -> bool {
        self.forget_canvas_captures();
        let Some(pending) = self.canvas_captures.get_mut(&ticket) else {
            return false;
        };
        let Ok(capture) = pending else { return false };
        match capture.canvas.snapshotted() {
            Ok(()) => {
                capture.copied = true;
                true
            }
            Err(refusal) => {
                *pending = Err(refusal);
                false
            }
        }
    }

    /// The presenter's PNG for capture `ticket` (empty: it could not encode
    /// one), written; or the refusal it handed back.
    pub fn canvas_captured(&mut self, ticket: u64, png: &[u8]) -> Outcome {
        self.forget_canvas_captures();
        let PendingCapture {
            canvas: c,
            scope,
            copied,
        } = match self.canvas_captures.remove(&ticket) {
            Some(Ok(pending)) => pending,
            Some(Err(refusal)) => return refusal,
            None => {
                return refused(
                    "InvalidStateError",
                    "no capture is pending under this ticket",
                )
            }
        };
        if !copied || png.is_empty() {
            return refused("EncodingError", "the canvas's bitmap could not be encoded");
        }
        let Some(roots) = self.app_roots() else {
            return refused(
                "NotSupportedError",
                "this host has no app files (no storage here)",
            );
        };
        match write(&roots, &self.grants(), scope.as_deref(), &c.path, png) {
            Ok(()) => c.captured(),
            Err((name, why)) => refused(name, &format!("{}: {why}", c.path)),
        }
    }
}

fn write(
    roots: &[std::path::PathBuf; 3],
    grants: &str,
    scope: Option<&str>,
    path: &str,
    png: &[u8],
) -> Result<(), (&'static str, String)> {
    use ibex2::boundary::HostError;
    let denied = |why: String| ("NotAllowedError", why);
    let failed = |e: HostError| match e {
        HostError::Denied { capability } => {
            denied(format!("no grant covers it; grant `{capability} {path}`"))
        }
        e => ("EncodingError", e.to_string()),
    };
    let admitted = exact_data::storage::scope(grants, scope).map_err(denied)?;
    let grants = ibex2::grant::GrantSet::parse(&exact_runner::io_grants(admitted))
        .map_err(|e| denied(format!("the app's grants did not parse: {e}")))?;
    for root in roots {
        std::fs::create_dir_all(root).map_err(|e| ("EncodingError", e.to_string()))?;
    }
    let directories = ibex2::stdlib::app_fs::AppDirectories::new(&roots[0], &roots[1], &roots[2])
        .map_err(failed)?;
    let mut staged = directories.stage(&grants, path).map_err(failed)?;
    staged.write_all(png).map_err(failed)?;
    staged.publish().map_err(failed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use exact_kernel::MonospaceMeasurer;
    use exact_plan::Value;
    use exact_runner::{Answer, DataError, Event, Request, Store};

    struct Source;
    impl DataSource for Source {
        fn grants(&self) -> &str {
            "fs.write app:/tmp"
        }
        fn canvas_surfaces(&self) -> Vec<(String, usize)> {
            vec![("ink".into(), 0)]
        }
        fn draw_2d(
            &mut self,
            _: &str,
            _: &[Value],
            ctx: &exact_runner::exact_canvas::Context2d,
            _: &exact_runner::exact_canvas::Frame,
        ) -> Result<bool, exact_runner::exact_canvas::DrawError> {
            ctx.fill_rect(0., 0., 4., 4.);
            Ok(true)
        }
        fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
            Err(DataError::UnknownSource(name.into()))
        }
        fn answer(&mut self, _: &mut Store, _: &str, args: &[Value]) -> Result<Answer, DataError> {
            if args.get(1) == Some(&Value::Bool(false)) {
                return Ok(Answer::Now(Value::str("hidden")));
            }
            Ok(Answer::Later(Request::capture_canvas(
                "ink",
                "app:/tmp/cancelled.png",
            )))
        }
    }

    #[test]
    fn forgotten_captures_release_their_records_and_reject_late_pngs() {
        let plan = contract::compile(
            r#"
component App
  state shown = true
  state step = 0
  resource image = capture(step, shown) as shape string
  action replace
    step = step + 1
  action hide
    shown = false
  view
    column
      button press=replace testId="replace"
        text "Replace"
      button press=hide testId="hide"
        text "Hide"
      if shown
        canvas surface=ink() width=40 height=40
        text image
"#,
        )
        .unwrap()
        .encode();
        let (mut host, _) = Host::boot(
            &plan,
            Source,
            Box::new(MonospaceMeasurer::default()),
            200.,
            200.,
        )
        .unwrap();
        host.activate_data();
        let id = |host: &Host<Source>, name: &str| {
            let kernel = host.runner().kernel();
            kernel
                .node_by_key(kernel.find_by_test_id(name)[0])
                .unwrap()
                .id
        };
        let replace = id(&host, "replace");
        let hide = id(&host, "hide");
        for n in 0..20 {
            let requests = host.take_requests();
            assert_eq!(requests.len(), 1);
            let ticket = requests[0].ticket;
            host.surface_work(&mut crate::batch::Batch::new(), &requests[0], None);
            assert_eq!(host.canvas_captures.len(), 1);
            assert!(
                host.canvas_captures[&ticket].is_ok(),
                "encoding was admitted"
            );
            if n % 2 == 0 {
                assert!(host.canvas_snapshotted(ticket));
            }
            host.dispatch(if n == 19 { hide } else { replace }, Event::Press);
            assert!(
                host.canvas_captures.is_empty(),
                "forgotten capture {n} is retained"
            );
            let outcome = host.canvas_captured(ticket, b"late PNG");
            assert!(exact_runner::canvas_capture::captured_image(&outcome)
                .unwrap_err()
                .starts_with("InvalidStateError:"));
        }
    }
    #[test]
    fn a_frames_capture_survives_its_animation_draw() {
        let plan = contract::compile("component App\n  mutation image as shape string\n  action capture\n    send image = capture()\n  task save mount\n    every(frame, capture)\n  view\n    canvas surface=ink() width=40 height=40\n").unwrap().encode();
        let (mut host, _) = Host::boot(
            &plan,
            Source,
            Box::new(MonospaceMeasurer::default()),
            200.,
            200.,
        )
        .unwrap();
        host.activate_data();
        host.take_requests();
        host.frame(16.);
        let request = host.take_requests().pop().unwrap();
        host.surface_work(&mut crate::batch::Batch::new(), &request, None);
        assert!(
            host.canvas_captures[&request.ticket].is_ok(),
            "this frame must remain capturable"
        );
    }
}
