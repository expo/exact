//! DOM's `beforeunload` as the person closes the window (studio diary R17),
//! as the Mac asks it (`UnloadMac.swift`): every mounted node that hears it
//! does, as every listener on the web's window does; one that calls
//! `preventDefault()` keeps the window, and the app asks its own question
//! ("Save changes?") and calls `close()` once it is answered, which closes
//! the window without asking again. A host with a window asks it before its
//! close (Windows, `host/windows/src/window/lifecycle.rs`).
use super::*;

impl<D: DataSource> Presenter<D> {
    /// Ask `beforeunload`, in view order: `false` when a handler called
    /// `preventDefault()` and the window stays.
    pub fn before_unload(&mut self, now: f64) -> bool {
        // An edit Windows' editor holds, from the same message batch as the
        // close, is the document's before the app is asked about it.
        #[cfg(windows)]
        self.windows_field_edits(now);
        let listeners: Vec<ViewId> = (self.host.runner().handlers().into_iter())
            .filter(|(_, kinds)| kinds.contains(&EventKind::Beforeunload))
            .map(|(view, _)| view)
            .collect();
        let mut prevented = false;
        for view in listeners {
            if self.host.kernel().node(view).is_none() {
                continue;
            }
            let from = self.commands.len();
            let error = self.host.dispatch_at(view, Event::Beforeunload, now);
            if let Some(error) = error.or(self.after_commit()) {
                self.host.log(error);
            }
            prevented |= self.prevented_since(from);
            // Every listener hears it: the window's is no bubbling event.
            self.commands.retain(|c| c.name != "stopPropagation");
        }
        if prevented {
            self.host.log("beforeunload: kept open");
        }
        !prevented
    }
}

#[cfg(test)]
mod tests {
    use crate::presenter::{PainterChoice, Presenter};
    use std::path::PathBuf;

    const APP: &str = r#"component App
  state unsaved = false
  state asked = 0
  action edit
    unsaved = true
  action ask
    asked = asked + 1
    if unsaved
      preventDefault()
  action other
    asked = asked + 10
  view
    main beforeunload=ask
      button press=edit testId="edit" height=30
        text "Edit"
      text asked.toString() testId="asked" beforeunload=other
"#;

    fn asked(p: &Presenter<()>) -> String {
        let kernel = p.host().kernel();
        let id = kernel
            .node_by_key(kernel.find_by_test_id("asked")[0])
            .unwrap()
            .id;
        kernel
            .node(id)
            .unwrap()
            .props
            .str(exact_kernel::PropId::Text)
            .unwrap_or_default()
            .to_owned()
    }

    #[test]
    fn every_listener_hears_it_and_one_that_prevents_it_keeps_the_window() {
        let (mut p, error) = Presenter::boot_with(
            &crate::test_plan(APP),
            (),
            (400., 800.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        assert!(p.before_unload(0.), "nothing unsaved: the window may close");
        assert_eq!(asked(&p), "11", "both listeners heard it");
        let kernel = p.host().kernel();
        let edit = kernel
            .node_by_key(kernel.find_by_test_id("edit")[0])
            .unwrap()
            .id;
        p.tap(edit).unwrap();
        p.run_commands(|| ());
        assert!(!p.before_unload(1.), "unsaved: the handler kept the window");
        assert_eq!(asked(&p), "22");
        let journal = crate::agent::handle(&mut p, r#"{"op":"logs"}"#);
        assert!(journal.contains("beforeunload: kept open"), "{journal}");
    }
}
