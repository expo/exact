//! The agent protocol's operations a production build uses, each its own
//! export (LLP 1047.001 D11), so that once the families' Swift calls these
//! the general dispatcher (`exact_agent`) and the operations only
//! development asks for can be linked in development alone. Each takes a
//! request in the input buffer, as `exact_agent` does, and answers what
//! that operation answers there.
use super::*;
use exact_runner::{agent, json};

impl<D: DataSource> Bridge<D> {
    /// The input buffer's first `len` bytes, answered by `f` on the booted
    /// host, with what [`Bridge::agent`] does around every operation.
    fn answer(&mut self, len: usize, f: impl FnOnce(&mut Host<D>, &str) -> String) -> u32 {
        let request =
            String::from_utf8_lossy(&self.input[..len.min(self.input.len())]).into_owned();
        let out = match self.host.as_mut() {
            Some(h) => {
                h.runner_mut().take_fault_logs();
                f(h, &request)
            }
            None => json::error("not booted"),
        };
        // An answered auth hold is delivered by the next pump (LLP 1069.006 D7).
        if out.contains("\"capability\":\"auth\"") {
            self.executor.as_deref().inspect(|x| x.notify());
        }
        self.emit(out)
    }

    /// The runner's outstanding work, which the launch ledger waits on.
    pub fn outstanding(&mut self, len: usize) -> u32 {
        self.answer(len, |h, _| {
            agent::tagged(h.runner(), h.runner().outstanding().json())
        })
    }

    /// `{"mustFinish":B}`: whether the app must finish work an answer
    /// started, which a quit waits for.
    pub fn must_finish(&mut self, len: usize) -> u32 {
        self.answer(len, |h, _| {
            let reply = format!("{{\"mustFinish\":{}}}", h.runner().must_finish());
            agent::tagged(h.runner(), reply)
        })
    }

    /// The background actions that ended since the last call, delivered.
    pub fn ended_actions(&mut self, len: usize) -> u32 {
        self.answer(len, |h, _| h.take_ended_actions())
    }

    /// What the app's literals declare: the notification categories a host
    /// registers at launch.
    pub fn declared(&mut self, len: usize) -> u32 {
        self.answer(len, |h, _| {
            agent::tagged(h.runner(), h.runner().declared_values().to_string())
        })
    }

    /// The view tree (`{"target":…}` for a subtree).
    pub fn tree(&mut self, len: usize) -> u32 {
        self.answer(len, |h, request| {
            agent::tagged(h.runner(), agent::tree_request(h.runner(), request))
        })
    }

    /// The runner's state.
    pub fn state(&mut self, len: usize) -> u32 {
        self.answer(len, |h, request| h.state_reply(request))
    }

    /// One view's rows (`{"id":V}`, `"plan":true` for the plan's digest).
    pub fn node(&mut self, len: usize) -> u32 {
        self.answer(len, |h, request| h.node_reply(request))
    }

    /// The work done by plan site (`perf`).
    pub fn perf(&mut self, len: usize) -> u32 {
        self.answer(len, |h, request| {
            agent::tagged(h.runner(), exact_runner::perf::reply(h.runner(), request))
        })
    }

    /// The journal from `{"since":N}`.
    pub fn logs(&mut self, len: usize) -> u32 {
        self.answer(len, |h, request| {
            agent::tagged(h.runner(), agent::logs_request(h.runner(), request))
        })
    }

    /// The session's identity: generation, epoch, incarnation and clock.
    pub fn tags(&mut self, len: usize) -> u32 {
        self.answer(len, |h, _| agent::tags(h.runner()))
    }

    /// What a file input's `showPicker` names (`{"id":I}`).
    pub fn picker(&mut self, len: usize) -> u32 {
        self.answer(len, |h, request| {
            agent::tagged(h.runner(), agent::picker(h.runner(), request))
        })
    }

    /// Hold a file input's picker for its answer (`{"id":I}` or `{"view":V}`).
    pub fn show_picker(&mut self, len: usize) -> u32 {
        self.answer(len, |h, request| {
            agent::show_picker(h.runner_mut(), request)
        })
    }

    /// Where a picked file named `{"name":N}` is copied in.
    pub fn picked_path(&mut self, len: usize) -> u32 {
        self.answer(len, |h, request| {
            crate::picker::with_files(agent::picked_path(h.runner_mut(), request))
        })
    }

    /// The app's file roots (`app:/data`, `app:/cache`, `app:/tmp`).
    pub fn app_roots(&mut self, len: usize) -> u32 {
        self.answer(len, |h, _| {
            crate::picker::app_roots_reply(h.runner().data_ref())
        })
    }

    /// Copy an app file a share names (`{"path":P,"to":T}`) for the sheet.
    pub fn share_file(&mut self, len: usize) -> u32 {
        let copy = self.links.io.map(|io| io.share_copy);
        self.answer(len, |h, request| {
            crate::picker::share_file(request, h.runner().data_ref(), copy)
        })
    }

    /// A documents operation (`{"op":…,"owner":O,…}`, LLP 1069.010 D1).
    pub fn documents(&mut self, len: usize) -> u32 {
        self.answer(len, |h, request| {
            let op = json::field_str(request, "op");
            crate::documents::answer(op.as_deref(), request, h.runner())
                .unwrap_or_else(|| json::error("not a documents operation"))
        })
    }

    /// A web view's dialog, file input, download or print, held for its
    /// answer (`{"node":N,"capability":C,"args":A}`).
    pub fn frame_request(&mut self, len: usize) -> u32 {
        self.answer(len, |h, request| {
            (h.runner_mut().hold_frame_request(request)).unwrap_or_else(|e| json::error(&e))
        })
    }

    /// A system's call of a tool (`{"op":"call"|"called"|"cancel",…}`, LLP
    /// 1133.023 §10), or a person's press of a control the binary compiled
    /// (`{"op":"press","control":K,"digest":D,"cold":B,"hidden":B}`, LLP
    /// 1126.004 D2), answered with the call's id as a call is. Only the
    /// tool door asks this export; the agent's operations reach `call_agent`
    /// and never `press`.
    pub fn tool(&mut self, len: usize) -> u32 {
        self.answer(len, |h, request| {
            let op = json::field_str(request, "op").unwrap_or_default();
            if op == "press" {
                return press(h.runner_mut(), request);
            }
            match h.runner_mut().call_agent(&op, request) {
                Some(reply) => reply.unwrap_or_else(|e| json::error(&e)),
                None => agent::tagged(h.runner(), json::error(&format!("unknown op: {op}"))),
            }
        })
    }

    /// Answer a held device request (`{"op":"tap","ticket":T,"choice":C}` or
    /// `{"op":"type","ticket":T,"text":V}`, LLP 1069.007 D4).
    pub fn answer_ticket(&mut self, len: usize) -> u32 {
        self.answer(len, |h, request| {
            let op = json::field_str(request, "op").unwrap_or_default();
            agent::answer_ticket(h.runner_mut(), &op, request).0
        })
    }
}

/// A press of a compiled control, as [`Bridge::tool`] takes it.
fn press<D: exact_runner::DataSource>(
    runner: &mut exact_runner::Runner<D>,
    request: &str,
) -> String {
    let v: serde_json::Value = serde_json::from_str(request).unwrap_or_default();
    let how = exact_runner::CallHow {
        door: "control".into(),
        cold: v["cold"] == true,
        hidden: v["hidden"] == true,
        digest: None,
    };
    let kind = v["control"].as_str().unwrap_or_default();
    let digest = v["digest"].as_str().unwrap_or_default();
    match runner.press_control(kind, digest, how) {
        Ok(id) => serde_json::json!({ "call": id, "control": kind }).to_string(),
        Err(e) => json::error(&e),
    }
}

/// The exports of [`Bridge`]'s operations above, which `host!`'s core
/// emits: each takes its request in the input buffer and returns the
/// reply's length.
#[macro_export]
macro_rules! answers_exports {
    () => {
        /// The launch ledger's outstanding work.
        #[no_mangle]
        pub extern "C" fn exact_outstanding(rt: u32, len: usize) -> u32 {
            $crate::abi::with_runtime(&EXACT_RUNTIMES, rt, true, |b, _| b.outstanding(len), |n| n)
        }
        /// Whether a quit waits for work an answer started.
        #[no_mangle]
        pub extern "C" fn exact_must_finish(rt: u32, len: usize) -> u32 {
            $crate::abi::with_runtime(&EXACT_RUNTIMES, rt, true, |b, _| b.must_finish(len), |n| n)
        }
        /// The background actions that ended, delivered.
        #[no_mangle]
        pub extern "C" fn exact_ended_actions(rt: u32, len: usize) -> u32 {
            $crate::abi::with_runtime(
                &EXACT_RUNTIMES,
                rt,
                true,
                |b, _| b.ended_actions(len),
                |n| n,
            )
        }
        /// The notification categories the app declares.
        #[no_mangle]
        pub extern "C" fn exact_declared(rt: u32, len: usize) -> u32 {
            $crate::abi::with_runtime(&EXACT_RUNTIMES, rt, true, |b, _| b.declared(len), |n| n)
        }
        /// The view tree.
        #[no_mangle]
        pub extern "C" fn exact_tree(rt: u32, len: usize) -> u32 {
            $crate::abi::with_runtime(&EXACT_RUNTIMES, rt, true, |b, _| b.tree(len), |n| n)
        }
        /// The runner's state.
        #[no_mangle]
        pub extern "C" fn exact_state(rt: u32, len: usize) -> u32 {
            $crate::abi::with_runtime(&EXACT_RUNTIMES, rt, true, |b, _| b.state(len), |n| n)
        }
        /// One view's rows.
        #[no_mangle]
        pub extern "C" fn exact_node(rt: u32, len: usize) -> u32 {
            $crate::abi::with_runtime(&EXACT_RUNTIMES, rt, true, |b, _| b.node(len), |n| n)
        }
        /// The work done by plan site.
        #[no_mangle]
        pub extern "C" fn exact_perf(rt: u32, len: usize) -> u32 {
            $crate::abi::with_runtime(&EXACT_RUNTIMES, rt, true, |b, _| b.perf(len), |n| n)
        }
        /// The journal.
        #[no_mangle]
        pub extern "C" fn exact_logs(rt: u32, len: usize) -> u32 {
            $crate::abi::with_runtime(&EXACT_RUNTIMES, rt, true, |b, _| b.logs(len), |n| n)
        }
        /// The session's identity.
        #[no_mangle]
        pub extern "C" fn exact_tags(rt: u32, len: usize) -> u32 {
            $crate::abi::with_runtime(&EXACT_RUNTIMES, rt, true, |b, _| b.tags(len), |n| n)
        }
        /// What a file input's `showPicker` names.
        #[no_mangle]
        pub extern "C" fn exact_picker(rt: u32, len: usize) -> u32 {
            $crate::abi::with_runtime(&EXACT_RUNTIMES, rt, true, |b, _| b.picker(len), |n| n)
        }
        /// Hold a file input's picker for its answer.
        #[no_mangle]
        pub extern "C" fn exact_show_picker(rt: u32, len: usize) -> u32 {
            $crate::abi::with_runtime(&EXACT_RUNTIMES, rt, true, |b, _| b.show_picker(len), |n| n)
        }
        /// Where a picked file is copied in.
        #[no_mangle]
        pub extern "C" fn exact_picked_path(rt: u32, len: usize) -> u32 {
            $crate::abi::with_runtime(&EXACT_RUNTIMES, rt, true, |b, _| b.picked_path(len), |n| n)
        }
        /// The app's file roots.
        #[no_mangle]
        pub extern "C" fn exact_app_roots(rt: u32, len: usize) -> u32 {
            $crate::abi::with_runtime(&EXACT_RUNTIMES, rt, true, |b, _| b.app_roots(len), |n| n)
        }
        /// Copy an app file a share names.
        #[no_mangle]
        pub extern "C" fn exact_share_file(rt: u32, len: usize) -> u32 {
            $crate::abi::with_runtime(&EXACT_RUNTIMES, rt, true, |b, _| b.share_file(len), |n| n)
        }
        /// A documents operation.
        #[no_mangle]
        pub extern "C" fn exact_documents(rt: u32, len: usize) -> u32 {
            $crate::abi::with_runtime(&EXACT_RUNTIMES, rt, true, |b, _| b.documents(len), |n| n)
        }
        /// A web view's request, held for its answer.
        #[no_mangle]
        pub extern "C" fn exact_frame_request(rt: u32, len: usize) -> u32 {
            $crate::abi::with_runtime(
                &EXACT_RUNTIMES,
                rt,
                true,
                |b, _| b.frame_request(len),
                |n| n,
            )
        }
        /// A system's call of a tool.
        #[no_mangle]
        pub extern "C" fn exact_tool(rt: u32, len: usize) -> u32 {
            $crate::abi::with_runtime(&EXACT_RUNTIMES, rt, true, |b, _| b.tool(len), |n| n)
        }
        /// Answer a held device request.
        #[no_mangle]
        pub extern "C" fn exact_answer_ticket(rt: u32, len: usize) -> u32 {
            $crate::abi::with_runtime(
                &EXACT_RUNTIMES,
                rt,
                true,
                |b, _| b.answer_ticket(len),
                |n| n,
            )
        }
    };
}

#[cfg(test)]
#[path = "answers_tests.rs"]
mod tests;
