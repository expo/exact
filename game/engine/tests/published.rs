//! A large public record (Grow a Garden's whole backpack, diary 004 limit 2):
//! a change to one small field re-encodes only that field, and the journal
//! keeps a bounded line for a large one.
use exact_game::*;

struct Empty;
impl Game for Empty {
    type Args = ();
    const ID: &'static str = "published";
    fn setup(_: &mut World, _: &()) {}
    fn tick(_: &mut World, _: &Input, _: &()) {}
}
#[derive(Default, Data, Clone)]
struct Fruit {
    id: u32,
    label: String,
    weight: f64,
    value: u32,
    mutated: bool,
}
#[derive(Default, Data)]
struct Hud {
    status: String,
    backpack: Vec<Fruit>,
}
fn backpack(n: u32) -> Vec<Fruit> {
    (0..n)
        .map(|id| Fruit {
            id,
            label: format!("Golden carrot {id}"),
            weight: 1.25 + id as f64 / 7.0,
            value: 30 + id % 17,
            mutated: id % 5 == 0,
        })
        .collect()
}

#[test]
fn unchanged_nan_is_published_once_across_ticks() {
    #[derive(Default, Data)]
    struct AccuracyHud {
        record_accuracy: f32,
    }
    struct Accuracy;
    impl Game for Accuracy {
        type Args = ();
        const ID: &'static str = "nan-publication";
        const HZ: u32 = 100;
        fn setup(_: &mut World, _: &()) {}
        fn tick(w: &mut World, _: &Input, _: &()) {
            let (hits, shots) = (0.0f32, 0.0f32);
            w.publish("accuracy", hits / shots);
            w.publish_record(&AccuracyHud {
                record_accuracy: hits / shots,
            });
            w.publish("steady", 1.0);
        }
    }
    let mut sim = Sim::<Accuracy>::new(()).unwrap();
    sim.take_published();
    let mut deliveries = Vec::new();
    for _ in 0..10 {
        assert_eq!(sim.run(10.0), 1);
        if let Some(record) = sim.take_published() {
            deliveries.push(record);
        }
    }
    let journal = sim.world().journal();
    let count = |key| {
        journal
            .iter()
            .filter(|event| event.line.contains(&format!("publish {key}:")))
            .count()
    };
    assert_eq!(
        (
            count("accuracy"),
            count("record_accuracy"),
            deliveries.len()
        ),
        (1, 1, 1)
    );
    assert_eq!(count("steady"), 1);
    assert_eq!(
        deliveries,
        [r#"{"accuracy":null,"record_accuracy":null,"steady":1}"#]
    );
}

#[test]
fn publication_journal_uses_delivered_json() {
    let mut sim = Sim::<Empty>::new(()).unwrap();
    for (value, json) in [
        (Value::Number(f64::NAN), "null"),
        (Value::Number(f64::INFINITY), "null"),
        (Value::Number(f64::NEG_INFINITY), "null"),
        (Value::Number(-0.0), "-0"),
        (Value::Number(1.23456789), "1.23456789"),
        (Value::str("quote \"\n"), r#""quote \"\n""#),
        (
            Value::record(vec![Value::Number(f64::NAN), Value::Bool(true)]),
            "[null,true]",
        ),
    ] {
        sim.world().publish("value", value);
        assert_eq!(sim.take_published(), Some(format!("{{\"value\":{json}}}")));
        let journal = sim.world().journal();
        assert!(
            journal
                .last()
                .unwrap()
                .line
                .ends_with(&format!("publish value: {json}")),
            "{}",
            journal.last().unwrap().line
        );
    }
    #[derive(Default, Data)]
    struct Readings {
        reading: Option<f64>,
        values: Vec<f32>,
    }
    #[derive(Default, Data)]
    struct Nested {
        metrics: Readings,
    }
    sim.world().publish_record(&Nested {
        metrics: Readings {
            reading: Some(f64::NAN),
            values: vec![f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.125],
        },
    });
    assert_eq!(
        sim.take_published().as_deref(),
        Some(r#"{"metrics":{"reading":null,"values":[null,null,null,0.125]},"value":[null,true]}"#)
    );
    assert!(sim
        .world()
        .journal()
        .last()
        .unwrap()
        .line
        .ends_with(r#"publish metrics: {"reading":null,"values":[null,null,null,0.125]}"#));
}

#[test]
fn publication_compares_numeric_bits_with_canonical_nan_recursively() {
    #[derive(Default, Data)]
    struct Readings {
        values: Vec<Option<f64>>,
    }
    #[derive(Default, Data)]
    struct Nested {
        metrics: Readings,
    }
    let mut sim = Sim::<Empty>::new(()).unwrap();
    sim.take_published();
    for (index, (number, changed)) in [
        (f64::NAN, true),
        (f64::from_bits(0x7ff0_0000_0000_0001), false),
        (f64::from_bits(0xfff8_0000_0000_0042), false),
        (f64::INFINITY, true),
        (f64::INFINITY, false),
        (f64::NEG_INFINITY, true),
        (f64::NEG_INFINITY, false),
        (1.25, true),
        (1.25, false),
        (1.25000001, true),
        (0.0, true),
        (-0.0, true),
        (-0.0, false),
        (f64::NAN, true),
        (f64::from_bits(0x7ff8_0000_0000_0042), false),
    ]
    .into_iter()
    .enumerate()
    {
        let epoch = sim.world().mutation_epoch();
        let journal = sim.world().journal_next();
        sim.world().publish("scalar", number);
        sim.world().publish_record(&Nested {
            metrics: Readings {
                values: vec![Some(number)],
            },
        });
        sim.world().publish(
            "positional",
            Value::record(vec![Value::list(vec![Value::Option(Some(
                std::rc::Rc::new(Value::Number(number)),
            ))])]),
        );
        assert_eq!(sim.take_published().is_some(), changed, "sample {index}");
        assert_eq!(
            sim.world().mutation_epoch() != epoch,
            changed,
            "sample {index}"
        );
        assert_eq!(
            sim.world().journal_next() - journal,
            if changed { 3 } else { 0 },
            "sample {index}"
        );
        if index == 0 {
            let saved = sim.save().unwrap();
            sim.restore(&saved).unwrap();
            assert!(sim.take_published().is_some());
        }
    }
}

#[test]
fn a_large_field_is_journaled_bounded_and_the_record_stays_exact() {
    let mut sim = Sim::<Empty>::new(()).unwrap();
    let hud = Hud {
        status: "day 1".into(),
        backpack: backpack(2_000),
    };
    sim.world().publish_record(&hud);
    let whole = sim.take_published().unwrap();
    assert!(whole.len() > 100_000, "{}", whole.len());
    assert!(whole.ends_with(r#"],"status":"day 1"}"#));
    let line = sim
        .world()
        .journal()
        .into_iter()
        .map(|e| e.line)
        .find(|l| l.contains("publish backpack:"))
        .unwrap();
    assert!(line.len() < 4_200, "journal line is {} bytes", line.len());
    assert!(line.ends_with(" bytes)"), "{}", &line[line.len() - 40..]);
    // One small field changes: the record is the old one with it replaced.
    sim.world().publish_record(&Hud {
        status: "day 2".into(),
        ..hud
    });
    let next = sim.take_published().unwrap();
    assert_eq!(next, whole.replace("day 1", "day 2"));
    // A save and restore carry the same record.
    let saved = sim.save().unwrap();
    let mut restored = Sim::<Empty>::new(()).unwrap();
    restored.restore(&saved).unwrap();
    assert_eq!(restored.take_published().as_deref(), Some(next.as_str()));
}

/// `cargo test --release -p exact-game --test published -- --ignored --nocapture`:
/// the garden's 69,327-fruit backpack (~6.4 MB) published beside a status
/// line that changes once a garden second; the time to take the record after
/// one status change.
#[test]
#[ignore = "measurement"]
fn status_change_beside_a_whole_backpack() {
    let mut sim = Sim::<Empty>::new(()).unwrap();
    let rows = backpack(69_327);
    sim.world().publish_record(&Hud {
        status: "t=0".into(),
        backpack: rows.clone(),
    });
    let bytes = sim.take_published().unwrap().len();
    let rounds = 20;
    let start = std::time::Instant::now();
    for i in 1..=rounds {
        sim.world().publish("status", format!("t={i}").as_str());
        assert!(sim.take_published().is_some());
    }
    let per = start.elapsed().as_secs_f64() * 1e3 / rounds as f64;
    println!("record {bytes} bytes: {per:.2} ms per status change");
}

/// `presented` is the host's member of the canvas's record (LLP 1009 D9).
#[test]
#[should_panic(expected = "`presented` is the host's field")]
fn presented_is_the_hosts_name() {
    World::new(60, 0).publish("presented", true);
}

#[test]
#[should_panic(expected = "`presented` is the host's field")]
fn a_record_cannot_carry_presented_either() {
    #[derive(Default, Data)]
    struct Shown {
        presented: bool,
    }
    World::new(60, 0).publish_record(&Shown::default());
}
