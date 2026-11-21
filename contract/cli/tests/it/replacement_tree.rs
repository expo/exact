//! LLP 1029.000.002 D3. A replacement batch must reproduce the candidate's
//! tree even when the two independently booted plans chose overlapping ids.

use exact_kernel::Kernel;
use exact_plan::Value;
use exact_runner::{Answer, DataError, DataSource, Request, Runner, Store};
use std::path::Path;

struct Pending;

impl DataSource for Pending {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        unreachable!("the corpus boots through answer")
    }

    fn answer(&mut self, _: &mut Store, source: &str, _: &[Value]) -> Result<Answer, DataError> {
        if source == "emptyPost" {
            return Ok(Answer::Now(Value::record(vec![
                Value::str(""),
                Value::str(""),
            ])));
        }
        Ok(Answer::Later(Request::get("https://fixture.invalid")))
    }
}

fn assert_tree(actual: &Kernel, expected: &Kernel, pair: (&str, &str)) {
    assert_eq!(actual.roots(), expected.roots(), "{pair:?}");
    assert_eq!(actual.live_count(), expected.live_count(), "{pair:?}");
    for id in expected.preorder_where(&expected.roots(), |_, _| true) {
        let a = actual
            .node(id)
            .unwrap_or_else(|| panic!("{pair:?} missing view {id}"));
        let b = expected.node(id).unwrap();
        assert_eq!(
            (a.node_type, a.parent),
            (b.node_type, b.parent),
            "{pair:?} {id}"
        );
        assert_eq!(a.children(), b.children(), "{pair:?} {id}");
        assert_eq!(a.props, b.props, "{pair:?} {id}");
        assert_eq!(a.style, b.style, "{pair:?} {id}");
        assert_eq!(a.looks(), b.looks(), "{pair:?} {id}");
    }
}

#[test]
fn applying_the_difference_reproduces_every_ordered_pair_of_corpus_trees() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../corpus");
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "contract"))
        .collect();
    entries.push(dir.join("use/app.contract"));
    entries.sort();
    let trees: Vec<_> = entries
        .iter()
        .map(|entry| {
            let plan = contract::compile_path(entry)
                .unwrap_or_else(|e| panic!("{}: {e}", entry.display()));
            let runner = Runner::boot(
                plan,
                Pending,
                Kernel::with_monospace(),
                Default::default(),
                "/",
            )
            .unwrap_or_else(|e| panic!("{}: {e:?}", entry.display()));
            (entry.strip_prefix(&dir).unwrap().to_str().unwrap(), runner)
        })
        .collect();
    for (from, old) in &trees {
        let mut actual = Kernel::with_monospace();
        actual
            .apply(0, 1, &old.kernel().replacement_ops(&actual))
            .unwrap();
        assert_tree(&actual, old.kernel(), ("empty", from));
        for (to, new) in &trees {
            let ops = new.kernel().replacement_ops(old.kernel());
            if from == to {
                assert!(ops.is_empty(), "{from}");
            }
            actual
                .apply(0, actual.epoch() + 1, &ops)
                .unwrap_or_else(|e| panic!("{from} -> {to}: {e:?}"));
            assert_tree(&actual, new.kernel(), (from, to));
            actual
                .apply(
                    0,
                    actual.epoch() + 1,
                    &old.kernel().replacement_ops(new.kernel()),
                )
                .unwrap();
            assert_tree(&actual, old.kernel(), (to, from));
        }
    }
}
