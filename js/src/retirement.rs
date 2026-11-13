//! Terminal heap exhaustion (LLP 1027 D10): retire one module, then load
//! its bytecode again when a new answer or canvas call asks for it.
use super::{DataError, Module};
use exact_runner::failure::FailureCode;

#[derive(Default)]
pub(crate) struct Retirement {
    pub reload: bool,
    pub message: Option<String>,
}

impl Module {
    pub(crate) fn retire_oom(&mut self) -> Option<DataError> {
        if !self
            .engine
            .as_ref()
            .is_some_and(|engine| engine.out_of_memory())
        {
            return None;
        }
        let message = format!(
            "held more than the {} MiB a data module's heap may hold",
            self.max_heap >> 20
        );
        self.identity.retire(message.clone());
        self.retirement.message = Some(message.clone());
        // Never drain a poisoned VM. Drop the adapter and its roots before
        // storage. Queued JS writes disappear; native operations already
        // submitted may finish on disk, but no completion crosses to the new VM.
        self.drop_runtime();
        self.watch.take();
        self.retirement.reload = true;
        self.host.storage_changed = self.directories.is_some();
        self.logs.push(format!(
            "data module retired: {message}; a new request starts a fresh runtime"
        ));
        Some(DataError::Failed(
            FailureCode::Error,
            message,
            String::new(),
        ))
    }

    pub(crate) fn after_oom<T>(&mut self, result: Result<T, DataError>) -> Result<T, DataError> {
        match self.retire_oom() {
            Some(error) => Err(error),
            None => result,
        }
    }

    pub(crate) fn owned_answer(
        &mut self,
        result: Result<exact_runner::Answer, DataError>,
    ) -> Result<exact_runner::Answer, DataError> {
        let mut answer = self.after_oom(result)?;
        if let exact_runner::Answer::Later(request) = &mut answer {
            request.module = Some(self.identity.instance());
        }
        Ok(answer)
    }

    pub(crate) fn reload_after_oom(&mut self) -> Result<(), DataError> {
        if self.retirement.reload && self.engine.is_none() {
            self.load().map_err(DataError::Unavailable)?;
            self.retirement.reload = false;
        }
        Ok(())
    }

    pub(crate) fn retired_reply(&self) -> Option<DataError> {
        self.retirement.reload.then(|| {
            DataError::Failed(
                FailureCode::Error,
                self.retirement.message.clone().unwrap_or_default(),
                String::new(),
            )
        })
    }
}

#[cfg(all(test, exact_js_engine))]
mod tests {
    use super::*;
    use exact_runner::{Answer, DataSource, Dispatch, Store};
    use std::time::Duration;

    #[test]
    #[cfg_attr(
        not(exact_js_oom),
        ignore = "requires an exception-on-OOM Hermes receipt"
    )]
    fn heap_oom_drops_queued_writes_but_a_submitted_write_may_finish() {
        ibex2_sqlite::link();
        let root = std::env::temp_dir().join(format!("exact-oom-storage-{}", std::process::id()));
        std::fs::create_dir_all(root.join("data")).unwrap();
        let database = rusqlite::Connection::open(root.join("data/notes.db")).unwrap();
        database
            .execute_batch("CREATE TABLE notes(body TEXT)")
            .unwrap();
        let grants = "net.fetch https://oom.test\nnet.websocket wss://oom.test\nfs.read app:/data\nfs.write app:/data\nsqlite.open app:/data/notes.db\n";
        let mut module = Module::new(
            include_bytes!(concat!(env!("OUT_DIR"), "/oom.hbc")).to_vec(),
            "test.oom",
            grants,
        );
        module.set_budget_ms(f64::INFINITY);
        module
            .configure_storage(root.join("data"), root.join("cache"), root.join("tmp"))
            .unwrap();
        module.bind(&contract::compile("component App\n  resource opened = open() as shape string\n  resource written = write() as shape string\n  resource exhausted = oom() as shape string\n  resource count = counter() as shape string\n  view\n    text count\n").unwrap());
        module.load().unwrap();
        let mut store = Store::new(grants, []);
        let mut answer = module.answer(&mut store, "open", &[]).unwrap();
        while let Answer::Later(request) = answer {
            let work = module.continuation(request.continuation.unwrap()).unwrap();
            answer = module.parse(&mut store, "open", &[], work()).unwrap();
        }
        database.execute_batch("BEGIN IMMEDIATE").unwrap();
        assert!(matches!(
            module.answer(&mut store, "write", &[]),
            Ok(Answer::Now(_))
        ));
        let context = module.storage.as_ref().unwrap().context.clone();
        assert_eq!(
            context.in_flight(),
            1,
            "one write submitted; the other waits in JS"
        );
        let _background = module
            .background(&store)
            .expect("the submitted write has a background round");
        let instance = module.identity.instance();
        let error = module.answer(&mut store, "oom", &[]).unwrap_err();
        assert!(format!("{error:?}").contains("held more than the 64 MiB"));
        assert!(!module.is_loaded());
        assert!(module.background(&store).is_none());
        assert!(instance
            .failure()
            .unwrap()
            .contains("held more than the 64 MiB"));
        database.execute_batch("COMMIT").unwrap();
        assert!(
            context.wait(Duration::from_secs(5)),
            "the old native write finishes without a VM"
        );
        let rows: Vec<String> = database
            .prepare("SELECT body FROM notes")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(rows, ["submitted"]);
        assert!(matches!(
            module.answer(&mut store, "counter", &[]),
            Ok(Answer::Now(_))
        ));
        assert!(module
            .background_landed(&store, Dispatch::again_outcome())
            .unwrap()
            .is_none());
        drop(module);
        drop(context);
        drop(database);
        std::fs::remove_dir_all(root).unwrap();
    }

    fn posts(adopt_oom: bool) -> (Module, Store) {
        let grants = "net.fetch https://oom.test\nnet.websocket wss://oom.test\nfs.read app:/data\nfs.write app:/data\nsqlite.open app:/data/notes.db\n";
        let mut module = Module::new(
            include_bytes!(concat!(env!("OUT_DIR"), "/oom.hbc")).to_vec(),
            "test.oom",
            grants,
        );
        module.set_budget_ms(f64::INFINITY);
        module.bind(&contract::compile("component App\n  resource result = post(0, 0) as shape string\n  resource exhausted = oom() as shape string\n  view\n    text result\n").unwrap());
        if adopt_oom {
            let engine = module
                .load_engine_with_prelude(include_bytes!(concat!(
                    env!("OUT_DIR"),
                    "/prelude-with-adoption-probe.hbc"
                )))
                .unwrap();
            module.install(engine).unwrap();
        } else {
            module.load().unwrap();
        }
        (module, Store::new(grants, []))
    }

    #[test]
    #[cfg_attr(
        not(exact_js_oom),
        ignore = "requires an exception-on-OOM Hermes receipt"
    )]
    fn heap_oom_keeps_the_adoption_that_failed_and_retires_all_kept_tokens() {
        use exact_runner::{InFlight, Target};
        let (mut module, mut store) = posts(true);
        let args: Vec<_> = (0..3)
            .map(|n| {
                vec![
                    exact_plan::Value::Number(n as f64),
                    exact_plan::Value::Number(0.),
                ]
            })
            .collect();
        let mut requests = Vec::new();
        for (n, args) in args.iter().enumerate() {
            let Answer::Later(request) = module
                .answer_for(Target::Resource(n), &mut store, "post", args)
                .unwrap()
            else {
                panic!("post")
            };
            requests.push(request);
        }
        module.forgotten(&store, &[]);
        let calls: Vec<_> = args
            .iter()
            .enumerate()
            .map(|(n, args)| InFlight {
                target: Target::Resource(n),
                source: "post",
                args,
                fetch: requests[n].fetch,
                continuation: None,
            })
            .collect();
        let instance = module.identity.instance();
        let kept = module.keep_sent(&store, &calls, false);
        assert!(!module.is_loaded());
        assert_eq!(kept, requests.iter().map(|r| r.fetch).collect::<Vec<_>>());
        assert!(instance
            .failure()
            .unwrap()
            .contains("held more than the 64 MiB"));
        for request in requests {
            let error = module
                .module_landed(
                    &mut store,
                    request.fetch.unwrap(),
                    Dispatch::again_outcome(),
                )
                .unwrap_err();
            assert!(
                error.to_string().contains("belongs to a retired runtime"),
                "{error}"
            );
        }
    }

    #[test]
    #[cfg_attr(
        not(exact_js_oom),
        ignore = "requires an exception-on-OOM Hermes receipt"
    )]
    fn heap_oom_fails_a_post_restored_after_an_equal_argument_refresh() {
        let plan = contract::compile(
            r#"
component App
  state fail = false
  resource posted = post(0, 0) as shape string
  resource exhausted = maybeOom(fail) as shape string
  resource invalid = refuse(fail) as shape string
  action replace
    refresh posted
    fail = true
  view
    text "alive"
"#,
        )
        .unwrap();
        let (mut module, _) = posts(false);
        module.bind(&plan);
        let mut r = exact_runner::Runner::boot(
            plan,
            module,
            exact_kernel::Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        let posts = r.take_requests();
        assert_eq!(posts.len(), 1);
        let original = posts[0].ticket;
        assert!(
            r.act("replace", vec![]).is_err(),
            "the unknown source refuses the commit"
        );
        assert_eq!(r.slot("fail"), Some(&exact_plan::Value::Bool(false)));
        assert!(r.holds(original), "rollback restores the original POST");
        let (ticket, outcome) = r
            .take_aborted_answer()
            .expect("the restored POST failed on retirement");
        assert_eq!(ticket, original);
        assert!(outcome.summary().contains("held more than the 64 MiB"));
        r.fulfill(ticket, outcome).unwrap();
        assert!(!r.holds(original));
        assert!(r
            .failed_resources()
            .iter()
            .any(|(name, error)| *name == "posted" && error.contains("held more than the 64 MiB")));
    }

    #[test]
    #[cfg_attr(
        not(exact_js_oom),
        ignore = "requires an exception-on-OOM Hermes receipt"
    )]
    fn heap_oom_already_clears_previously_kept_tokens() {
        let (mut module, mut store) = posts(false);
        module.work.keep_ticket(700);
        let instance = module.identity.instance();
        assert!(module.answer(&mut store, "oom", &[]).is_err());
        assert!(instance
            .failure()
            .unwrap()
            .contains("held more than the 64 MiB"));
        let error = module
            .module_landed(&mut store, 700, Dispatch::again_outcome())
            .unwrap_err();
        assert!(
            error.to_string().contains("belongs to a retired runtime"),
            "{error}"
        );
    }

    #[test]
    fn canvas_reload_failure_reaches_the_draw_reply() {
        let (mut module, _) = posts(false);
        module.unload();
        module.retirement.reload = true;
        module.bytecode =
            super::super::Bytecode::Owned(std::sync::Arc::from(&b"invalid bytecode"[..]));
        let request = exact_runner::DrawRequest {
            canvas: 1,
            generation: 1,
            seq: 1,
            surface: "test",
            args: &[],
            names: &[],
            frame: Default::default(),
            current_color: None,
            rtl: false,
            p3: false,
        };
        let expected = module.reload_after_oom().unwrap_err().to_string();
        let exact_runner::Drawn::Now(reply) =
            module.draw(&request, &exact_runner::exact_canvas::Context2d::new())
        else {
            panic!("synchronous draw")
        };
        assert_eq!(reply.error.as_deref(), Some(expected.as_str()));
    }
    #[test]
    #[cfg_attr(
        not(exact_js_oom),
        ignore = "requires an exception-on-OOM Hermes receipt"
    )]
    fn heap_oom_during_adoption_fails_every_kept_fetch_before_its_reply() {
        let plan = contract::compile(
            r#"
component App
  state n = 0
  resource first = post(0, n) as shape string
  resource second = post(1, n) as shape string
  resource third = post(2, n) as shape string
  action replace
    n = 1
  view
    text "alive"
"#,
        )
        .unwrap();
        let (mut m, _) = posts(true);
        m.bind(&plan);
        let mut r = exact_runner::Runner::boot(
            plan,
            m,
            exact_kernel::Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        let posts = r.take_requests();
        assert_eq!(posts.len(), 3);
        r.act("replace", vec![]).unwrap();
        for post in &posts {
            // No collect_aborts first: a host reply may arrive immediately after the commit.
            r.fulfill(post.ticket, Dispatch::again_outcome()).unwrap();
        }
        let logs = exact_runner::agent::logs(&r, 0);
        for post in &posts {
            assert!(
            logs.contains(&format!(
                "request {}: executor retired: held more than the 64 MiB a data module's heap may hold",
                post.ticket
            )),
            "{logs}"
        );
        }
        assert!(r.pending().is_empty());
    }
}
