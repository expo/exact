//! @ref LLP 1094.000 D2 — the file-entry rule a drag and `share` share.
//! A shipped `assets/` file needs no grant; an `app:/` file is a plain app
//! path the `fs.read` grants cover, as `share(files=)` has always read one
//! (LLP 1125 D12). A drag admits both; `share` admits `app:/` files and an
//! element's picture (`share.rs`). Admission reads no bytes.

use crate::save_file::plain_app_path;
use crate::{DataSource, Runner};

/// An `app:/` entry the app may read.
pub(crate) fn app_file<D: DataSource>(runner: &Runner<D>, entry: &str) -> Result<(), String> {
    if !plain_app_path(entry) {
        return Err(format!("{entry} is not a plain app:/ path"));
    }
    if !runner.covers("fs.read", entry) {
        return Err(format!("{entry} is outside the app's fs.read grants"));
    }
    Ok(())
}

/// A drag's entry on this host: a shipped file, or an `app:/` file the
/// grants cover where the host can read one as the drag starts, which the
/// browser cannot inside its `dragstart`.
pub(crate) fn drag_file<D: DataSource>(
    runner: &Runner<D>,
    entry: &str,
    web: bool,
) -> Result<(), String> {
    if exact_plan::is_shipped_drag_file(entry) {
        return Ok(());
    }
    if !entry.starts_with("app:/") {
        return Err(format!(
            "{entry} is neither a shipped assets/ file nor an app:/ path"
        ));
    }
    if web {
        return Err(format!(
            "{entry}: an app:/ file cannot be dragged on the web"
        ));
    }
    app_file(runner, entry)
}
