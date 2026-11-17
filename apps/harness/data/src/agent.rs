//! The agent loop, on its own thread per turn: ask the model with the
//! conversation and the tools, stream its text into a busy assistant entry,
//! run the tools it calls (waiting for approval where one is needed), and
//! ask again until it answers without tools. An interrupt settles the
//! turn's entries from the caller's thread; this thread then finds its turn
//! is no longer current and leaves without touching them. The turn stays
//! the running one, and the session busy, until this thread has left: a
//! host waiting for the app to finish (LLP 1101.003 D2's shutdown) waits
//! for the tool it was running to stop.

use crate::providers::{self, Ask, Route};
use crate::settle::{Reply, TailUpdate};
use crate::sse::Event;
use crate::state::{Block, Choice, Entry, Line, Msg, Part, Role, Run, Shared, State, Turn};
use crate::tools;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// A turn never runs more model rounds than this.
const MAX_ROUNDS: usize = 40;

/// Begin a turn for `prompt`, under the same lock that saw no turn running
/// and took the prompt from the queue: the turn it installs is the one its
/// worker runs. `None` when the model cannot be reached.
fn begin(s: &mut State, prompt: &str) -> Option<(Turn, Route)> {
    s.push(Entry {
        kind: "user".into(),
        blocks: vec![Block::lines(
            prompt
                .lines()
                .map(|l| Line::plain(tools::clean_line(l)))
                .collect(),
        )],
        ..Entry::default()
    });
    // The model is captured with the route: what this turn's entries
    // say answered, even if the picker changes mid-turn (P12).
    let model = s.model_label();
    match providers::route(&s.model, &s.provider(), &s.sources()) {
        Err(e) => {
            let message = format!("Can't reach {}: {e}", s.model);
            let id = s.error_scrubbed(&message);
            if let Some(e) = s.entries.last_mut().filter(|e| e.id == id) {
                e.model = model;
            }
            None
        }
        Ok(route) => {
            s.failed = false;
            s.history.push(Msg::user(prompt));
            s.next_turn += 1;
            let turn = Turn {
                id: s.next_turn,
                cancel: Arc::new(AtomicBool::new(false)),
                model,
            };
            s.turn = Some(turn.clone());
            s.phase = "thinking".into();
            Some((turn, route))
        }
    }
}

fn run(shared: &Arc<Shared>, turn: &Turn, route: &Route) {
    let mut done = false;
    let mut stopped = false;
    for _ in 0..MAX_ROUNDS {
        match round(shared, turn, route) {
            Some(true) => continue,
            Some(false) => {
                done = true;
                break;
            }
            None => {
                stopped = true;
                break;
            }
        }
    }
    let mut s = shared.lock();
    if !stopped && s.current(turn) {
        if !done {
            s.notice(vec![Block::p(vec![Run::dim(format!(
                "Stopped after {MAX_ROUNDS} rounds of tool calls."
            ))])]);
        }
        end(&mut s);
        drop(s);
        shared.changed();
        pump(shared);
        return;
    }
    drop(s);
    release(shared, turn);
}

/// The thread of an interrupted turn has left: the turn ends here, not at
/// the interrupt. A prompt queued meanwhile then starts, except in a run
/// nobody attends, where the only interrupt is the host's shutdown.
pub(crate) fn release(shared: &Arc<Shared>, turn: &Turn) {
    let unattended = {
        let mut s = shared.lock();
        if s.turn.as_ref().map(|t| t.id) != Some(turn.id) {
            return;
        }
        s.turn = None;
        s.unattended
    };
    shared.changed();
    if !unattended {
        pump(shared);
    }
}

/// Start the queued prompts, oldest first, while no turn runs (P16).
pub fn pump(shared: &Arc<Shared>) {
    loop {
        let started = {
            let mut s = shared.lock();
            if s.busy() {
                return;
            }
            let Some(prompt) = s.queue.pop_front() else {
                return;
            };
            begin(&mut s, &prompt)
        };
        shared.changed();
        if let Some((turn, route)) = started {
            let shared = shared.clone();
            std::thread::spawn(move || run(&shared, &turn, &route));
            return;
        }
    }
}

/// Finish the turn: nothing busy, nothing waiting.
fn end(s: &mut State) {
    s.turn = None;
    settle_turn(s);
}

/// Everything [`end`] does but end the turn: what an interrupt does while
/// the turn's thread is still leaving.
fn settle_turn(s: &mut State) {
    s.phase = "idle".into();
    s.approval = Default::default();
    s.decision = None;
    if let Some((mut assistant, results)) = s.round.take() {
        if assistant.parts.is_empty() && !s.round_text.is_empty() {
            assistant
                .parts
                .push(Part::Text(std::mem::take(&mut s.round_text)));
        }
        finish_round(s, assistant, results);
    }
}

/// Put a round into the history; a tool call with no result gets one.
fn finish_round(s: &mut State, assistant: Msg, mut results: Vec<Part>) {
    let calls: Vec<String> = assistant
        .parts
        .iter()
        .filter_map(|p| match p {
            Part::ToolUse { id, .. } => Some(id.clone()),
            _ => None,
        })
        .collect();
    if assistant.parts.is_empty() {
        return;
    }
    s.history.push(assistant);
    for id in calls {
        let answered = results
            .iter()
            .any(|r| matches!(r, Part::ToolResult { id: rid, .. } if *rid == id));
        if !answered {
            results.push(Part::ToolResult {
                id,
                content: "interrupted by user".into(),
                is_error: true,
            });
        }
    }
    if !results.is_empty() {
        s.history.push(Msg {
            role: Role::User,
            parts: results,
        });
    }
}

/// Interrupt the running turn, from any thread: its busy entries settle
/// with a dim "[interrupted]" note, and the turn ends once its thread has
/// left ([`release`]). Whether one was running.
pub fn interrupt(shared: &Arc<Shared>) -> bool {
    let mut s = shared.lock();
    let Some(turn) = s.turn.clone() else {
        return false;
    };
    turn.cancel.store(true, Ordering::SeqCst);
    for e in s.entries.iter_mut().filter(|e| e.busy) {
        if e.kind == "tool" {
            e.status = "error".into();
        }
        e.blocks.push(Block::p(vec![Run::dim("[interrupted]")]));
        e.busy = false;
    }
    settle_turn(&mut s);
    drop(s);
    shared.wake.notify_all();
    shared.changed();
    true
}

/// One model round: `Some(true)` to ask again (tools ran), `Some(false)`
/// when the model is done, `None` when the turn stopped being current.
fn round(shared: &Arc<Shared>, turn: &Turn, route: &Route) -> Option<bool> {
    let (history, system, base_in, base_out) = {
        let mut s = shared.lock();
        if !s.current(turn) {
            return None;
        }
        s.phase = "thinking".into();
        s.round_text.clear();
        s.round = Some((
            Msg {
                role: Role::Assistant,
                parts: vec![],
            },
            vec![],
        ));
        (
            s.history.clone(),
            providers::system(&s.cwd),
            s.tokens_in,
            s.tokens_out,
        )
    };
    shared.changed();
    let mut reply = Reply::default();
    let mut tail: Option<String> = None;
    let mut settled = 0usize;
    let mut calls: Vec<(String, String, serde_json::Value)> = Vec::new();
    let mut stale = false;
    let ask = Ask {
        system: &system,
        history: &history,
        cancel: &turn.cancel,
    };
    let result = providers::stream(route, &ask, &mut |event| {
        if stale {
            return;
        }
        match event {
            Event::Text(t) => {
                let step = reply.push(&t);
                let crate::settle::Step {
                    finished,
                    tail: update,
                } = step;
                let mut s = shared.lock();
                if !s.current(turn) {
                    stale = true;
                    return;
                }
                s.round_text.push_str(&t);
                for blocks in finished {
                    settle(&mut s, &mut tail, &mut settled, blocks);
                }
                show_tail(&mut s, &mut tail, settled, update);
                s.phase = "streaming".into();
                drop(s);
                shared.changed();
            }
            Event::ToolCall { id, name, input } => calls.push((id, name, input)),
            Event::Usage { input, output } => {
                let mut s = shared.lock();
                if !s.current(turn) {
                    stale = true;
                    return;
                }
                if let Some(i) = input {
                    s.context = i;
                    s.tokens_in = base_in + i;
                }
                if let Some(o) = output {
                    s.tokens_out = base_out + o;
                }
                drop(s);
                shared.changed();
            }
            Event::Done | Event::Error(_) => {}
        }
    });
    let mut s = shared.lock();
    if !s.current(turn) {
        return None;
    }
    // The tail settles as the reply's last block, or goes if it is empty.
    let last = reply.finish();
    if let Some(id) = tail.take() {
        if last.is_empty() {
            s.entries.retain(|e| e.id != id || !e.busy);
        } else if let Some(e) = s.busy_entry(&id) {
            e.blocks = last;
            e.busy = false;
        }
    }
    if let Err(e) = result {
        s.turn_error(&e);
        end(&mut s);
        drop(s);
        shared.changed();
        pump(shared);
        return None;
    }
    let text = reply.text().to_string();
    let parts: Vec<Part> = (!text.is_empty())
        .then(|| Part::Text(text.clone()))
        .into_iter()
        .chain(calls.iter().map(|(id, name, input)| Part::ToolUse {
            id: id.clone(),
            name: name.clone(),
            input: input.clone(),
        }))
        .collect();
    if let Some((msg, _)) = s.round.as_mut() {
        msg.parts = parts;
    }
    drop(s);
    shared.changed();
    if calls.is_empty() {
        return Some(false);
    }
    for (id, name, input) in calls {
        let result = tool(shared, turn, &id, &name, &input)?;
        let mut s = shared.lock();
        if !s.current(turn) {
            return None;
        }
        if let Some((_, results)) = s.round.as_mut() {
            results.push(result);
        }
    }
    let mut s = shared.lock();
    if !s.current(turn) {
        return None;
    }
    if let Some((assistant, results)) = s.round.take() {
        finish_round(&mut s, assistant, results);
    }
    Some(true)
}

/// Settle one finished group: the busy tail becomes it, or it is pushed
/// settled. The reply's first block is "assistant", every later one "more".
fn settle(s: &mut State, tail: &mut Option<String>, settled: &mut usize, blocks: Vec<Block>) {
    let kind = if *settled == 0 { "assistant" } else { "more" };
    *settled += 1;
    if let Some(e) = tail.take().as_deref().and_then(|id| s.busy_entry(id)) {
        e.blocks = blocks;
        e.busy = false;
        return;
    }
    let model = s.turn_model();
    s.push(Entry {
        kind: kind.into(),
        blocks,
        model,
        ..Entry::default()
    });
}

/// Show the unfinished blocks in the one busy tail entry.
fn show_tail(s: &mut State, tail: &mut Option<String>, settled: usize, update: TailUpdate<'_>) {
    if let Some(e) = tail.as_deref().and_then(|id| s.busy_entry(id)) {
        update.apply(&mut e.blocks);
        return;
    }
    let blocks = update.into_blocks();
    let model = s.turn_model();
    let id = s.push(Entry {
        kind: if settled == 0 { "assistant" } else { "more" }.into(),
        busy: true,
        blocks,
        model,
        ..Entry::default()
    });
    *tail = Some(id);
}

fn body(lines: Vec<Line>) -> (Vec<Block>, f64) {
    let more = lines.len().saturating_sub(tools::SHOWN);
    let shown = lines.into_iter().take(tools::SHOWN).collect();
    (vec![Block::lines(shown)], more as f64)
}

/// Run one call, asking first when it needs approval; its result for the
/// model, or `None` when the turn stopped.
fn tool(
    shared: &Arc<Shared>,
    turn: &Turn,
    call: &str,
    name: &str,
    input: &serde_json::Value,
) -> Option<Part> {
    let title = tools::title(name, input);
    let needs = tools::needs_approval(name) && crate::sse::invalid_arguments(input).is_none();
    let preview = needs.then(|| tools::preview(name, input));
    let entry = {
        let mut s = shared.lock();
        if !s.current(turn) {
            return None;
        }
        let ask = needs && !s.always.iter().any(|t| t == name);
        if ask && s.unattended {
            // Nobody can answer (LLP 1101.003 D8): refused, not awaited,
            // and the turn stops, so a script knows the answer is
            // incomplete.
            s.push(Entry {
                kind: "tool".into(),
                title,
                status: "denied".into(),
                blocks: vec![Block::lines(vec![Line {
                    runs: vec![Run::dim("needs approval")],
                }])],
                link: tools::file_link(name, input),
                model: turn.model.clone(),
                ..Entry::default()
            });
            s.turn_error(&format!(
                "`{name}` needs approval, which a run from the command line cannot give."
            ));
            end(&mut s);
            drop(s);
            shared.changed();
            return None;
        }
        let id = s.push(Entry {
            kind: "tool".into(),
            busy: true,
            title,
            status: if ask { "waiting" } else { "running" }.into(),
            link: tools::file_link(name, input),
            model: turn.model.clone(),
            ..Entry::default()
        });
        if ask {
            let (summary, lines) = preview
                .as_ref()
                .map(|(s, l, _)| (s.clone(), l.clone()))
                .unwrap_or_default();
            s.approval = crate::state::Approval {
                id: id.clone(),
                tool: name.into(),
                summary,
                lines,
            };
            s.decision = None;
            s.phase = "waiting".into();
        } else {
            s.phase = "tool".into();
        }
        (id, ask)
    };
    shared.changed();
    let (id, ask) = entry;
    if ask {
        let choice = {
            let mut s = shared.lock();
            loop {
                if !s.current(turn) {
                    return None;
                }
                if s.decision.as_ref().is_some_and(|(d, _)| *d == id) {
                    break s.decision.take().map(|d| d.1).unwrap_or(Choice::No);
                }
                s = shared
                    .wake
                    .wait_timeout(s, Duration::from_millis(100))
                    .unwrap_or_else(|e| e.into_inner())
                    .0;
            }
        };
        let mut s = shared.lock();
        if !s.current(turn) {
            return None;
        }
        s.approval = Default::default();
        match choice {
            Choice::No => {
                if let Some(e) = s.busy_entry(&id) {
                    e.status = "denied".into();
                    e.blocks = vec![Block::lines(vec![Line {
                        runs: vec![Run::dim("denied by user")],
                    }])];
                    e.busy = false;
                }
                s.phase = "thinking".into();
                drop(s);
                shared.changed();
                return Some(Part::ToolResult {
                    id: call.into(),
                    content: "denied by user".into(),
                    is_error: true,
                });
            }
            Choice::Always => s.always.push(name.into()),
            Choice::Yes => {}
        }
        if let Some(e) = s.busy_entry(&id) {
            e.status = "running".into();
        }
        s.phase = "tool".into();
        drop(s);
        shared.changed();
    }
    let done = if needs && matches!(name, "write_file" | "edit_file") {
        match preview.as_ref().and_then(|(_, _, edit)| edit.as_ref()) {
            Some(Ok(edit)) => tools::run_prepared(name, edit, &turn.cancel),
            Some(Err(error)) => tools::Done::error(error.clone()),
            None => tools::Done::error("cannot prepare approved edit"),
        }
    } else {
        tools::run(name, input, &turn.cancel)
    };
    let mut s = shared.lock();
    if !s.current(turn) {
        return None;
    }
    if let Some(e) = s.busy_entry(&id) {
        e.status = if done.is_error { "error" } else { "ok" }.into();
        (e.blocks, e.more) = body(done.lines);
        e.busy = false;
    }
    s.phase = "thinking".into();
    drop(s);
    shared.changed();
    Some(Part::ToolResult {
        id: call.into(),
        content: done.text,
        is_error: done.is_error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown;

    #[test]
    fn live_code_update_falls_back_for_replaced_or_missing_busy_entries() {
        let mut reply = Reply::default();
        let mut state = State::default();
        let mut tail = None;

        let update = reply.push("```rust\nfn").tail;
        show_tail(&mut state, &mut tail, 0, update);
        let id = tail.clone().expect("tail entry");
        assert_eq!(state.busy_entry(&id).unwrap().blocks[0].kind, "code");

        state.busy_entry(&id).unwrap().blocks = vec![Block::p(vec![Run::plain("replaced")])];
        let update = reply.push(" main() {}\n").tail;
        show_tail(&mut state, &mut tail, 0, update);
        assert_eq!(
            state.busy_entry(&id).unwrap().blocks,
            markdown::streaming_blocks("```rust\nfn main() {}\n")
        );

        state.entries.clear();
        let update = reply.push("let x = 1;\n").tail;
        show_tail(&mut state, &mut tail, 0, update);
        let recreated = tail.as_deref().and_then(|id| state.busy_entry(id)).unwrap();
        assert_eq!(
            recreated.blocks,
            markdown::streaming_blocks("```rust\nfn main() {}\nlet x = 1;\n")
        );
    }
}
