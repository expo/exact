//! An attached session's reads (LLP 1012.000 D3), for the Linux attach carrier.
use crate::presenter::Presenter;
use exact_runner::json::{error, field_bool, field_num, field_str};
use exact_runner::DataSource;

/// An attached session's read: `tree`, `state`, `layout`, `logs`, `tags` and `perf <target>` from what the
/// presenter holds, with no pump, command, frame, clock step or guest call, which the stdio carrier's `handle`
/// makes before each request. `state` is the runner's projection alone, masked whole: the presenter's own
/// sections (a field's decorations carry its text) and a canvas's surface state are left out. `line` is a
/// request the attach carrier built from checked fields; any other op is refused.
pub fn passive<D: DataSource>(p: &mut Presenter<D>, line: &str) -> String {
    match field_str(line, "op").as_deref() {
        Some("tree") => super::tree(p, line),
        Some("state") if !field_bool(line, "reveal") => {
            let runner = p.host().runner();
            exact_runner::agent::state_at(runner, line, runner.now_ms())
        }
        Some("layout") => p.layout_held(
            field_num(line, "id").map(|n| n as u32),
            field_bool(line, "plan"),
        ),
        Some("logs" | "tags") => p.host().agent(line),
        Some("perf") if !field_bool(line, "frames") && !field_bool(line, "hatches") => {
            p.hatch_sites(p.host().agent(line))
        }
        _ => error("unsupported: not a read an attached session makes"),
    }
}
