use super::{assert_replacement_refused, boot_with, Desk, Value, APP};
use crate::source::Read;

#[test]
fn a_wrong_type_replacement_is_refused_without_walking_shared_children() {
    check_replacement_deadline(|| assert_replacement_refused("room:shared-list-merge"));
}

#[test]
fn a_typed_replacement_with_an_oversized_shared_leaf_is_refused_promptly() {
    check_replacement_deadline(|| {
        let src = APP.replace(
            "  view\n",
            concat!(
                "  action appendPair(earlier: string[][][][], later: string[][][][])\n",
                "    send rec = save(\"first\")\n",
                "    send rec = appendNested(earlier)\n",
                "    send rec = appendNested(later)\n",
                "  view\n",
            ),
        );
        let mut r = boot_with(
            &src,
            Desk {
                replace_queue_tail: true,
                ..Desk::default()
            },
        );
        let empty = Value::list(vec![]);
        r.act("appendPair", vec![empty.clone(), Value::list(vec![empty])])
            .unwrap();
        r.advance(0.0).unwrap();
        assert_eq!(
            r.source::<Desk>().asks,
            ["save:first", "appendNested:0", "appendNested:1"]
        );
    });
}

fn check_replacement_deadline(run: impl FnOnce() + Send + 'static) {
    let (done, completed) = std::sync::mpsc::channel();
    let check = std::thread::spawn(move || {
        run();
        done.send(()).unwrap();
    });
    let result = completed.recv_timeout(std::time::Duration::from_secs(5));
    assert_ne!(
        result,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout),
        "replacement check exceeded its five-second deadline"
    );
    check.join().unwrap();
}
