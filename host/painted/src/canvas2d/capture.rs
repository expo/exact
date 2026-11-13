//! A 2D canvas's kept bitmap as a PNG file (LLP 1056 D9, as amended for
//! expo/exact#188; `exact_runner::canvas_capture`). The bitmap is the
//! replayer's own pixmap, every list applied (the painters show a copy of
//! it), so an accumulating drawing, its images and its text are what the
//! capture holds. tiny-skia's encoder writes it straight-alpha RGBA8, which
//! a PNG without a colour chunk is read as sRGB. The file is written beside
//! its target and renamed over it, under `fs.write`, as `exactBodyTo` writes.

use super::Canvases;
use exact_runner::canvas_capture::{refused, CanvasCapture};
use exact_runner::Outcome;

impl Canvases {
    /// Encode only the bitmap held for the requested lifetime and generation.
    fn capture_png(&self, c: &CanvasCapture) -> Result<Vec<u8>, String> {
        if c.sequence == 0
            || self
                .completed
                .get(&c.view)
                .is_none_or(|seq| *seq < c.sequence)
        {
            return Err("the requested drawing has not been replayed".into());
        }
        let pixmap = match self.replayers.get(&c.view) {
            Some(r) if r.lifetime == c.lifetime && r.generation == c.generation => &r.pixmap,
            _ => return Err("the requested canvas generation has not been drawn".into()),
        };
        pixmap.encode_png().map_err(|e| e.to_string())
    }
}

/// The capture `found` (the runner's resolution of the request) carried
/// out: the PNG written at its path under the app's `grants`, narrowed by
/// the request's `scope`, below the app's `roots`.
pub(crate) fn capture(
    canvases: &Canvases,
    found: Result<CanvasCapture, Outcome>,
    roots: Option<[std::path::PathBuf; 3]>,
    grants: &str,
    scope: Option<&str>,
) -> Outcome {
    let c = match found {
        Ok(c) => c,
        Err(refusal) => return refusal,
    };
    let Some(roots) = roots else {
        return refused(
            "NotSupportedError",
            "this host has no app files (no storage here)",
        );
    };
    let png = match canvases.capture_png(&c) {
        Ok(png) => png,
        Err(why) => return refused("EncodingError", &why),
    };
    if let Err(refusal) = c.snapshotted() {
        return refusal;
    }
    match write(&roots, grants, scope, &c.path, &png) {
        Ok(()) => c.captured(),
        Err((name, why)) => refused(name, &format!("{}: {why}", c.path)),
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
