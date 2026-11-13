//! One retained curated document plus the existing latest-only stress source.
use exact_plan::Value;
use exact_runner::{Answer, DataError, DataSource, Outcome, Store};

#[cfg(target_arch = "wasm32")]
type Source = markdown_stress_data::MarkdownStress;
#[cfg(not(target_arch = "wasm32"))]
type Source = markdown_stress_data::NativeMarkdownStress;

pub struct Runbook {
    curated: Value,
    stress: Source,
}

impl Default for Runbook {
    fn default() -> Self {
        Self {
            curated: document(include_str!("../../runbook.md")),
            stress: Source::default(),
        }
    }
}

fn document(source: &str) -> Value {
    let parsed = markdown_parse::parse(source, &str::to_owned);
    Value::record(vec![
        Value::str(&parsed.title),
        Value::Number(source.len() as f64),
        markdown_parse::value::into_blocks_with_tables(parsed),
    ])
}

#[test]
fn runbook_tables_keep_the_header_and_styled_cells_in_one_block() {
    fn fields(value: &Value) -> &[Value] {
        match value {
            Value::Record(fields) | Value::List(fields) => fields,
            _ => panic!("record or list"),
        }
    }
    fn run(cell: &Value) -> &[Value] {
        fields(&fields(&fields(cell)[1])[0])
    }
    let Value::Record(book) = document(
        "# Runbook\n\n| LongUnbreakableHeader | B |\n| --- | --- |\n| **ready** | `code` |\n| [link](#next) | uneven content |\n\n## Next\n",
    ) else {
        panic!("runbook record")
    };
    let Value::List(blocks) = &book[2] else {
        panic!("blocks")
    };
    assert_eq!(blocks.len(), 3);
    let Value::Record(table) = &blocks[1] else {
        panic!("table block")
    };
    assert_eq!(table[1], Value::str("table"));
    assert_eq!(table[2], Value::Number(2.));
    let Value::List(cells) = &table[8] else {
        panic!("cells")
    };
    assert_eq!(cells.len(), 6);
    assert_eq!(run(&cells[2])[2], Value::Number(700.));
    assert_eq!(run(&cells[3])[4], Value::Bool(true));
    assert_eq!(run(&cells[4])[5], Value::str("#next"));
}

fn args(values: &[Value]) -> Result<Option<Vec<Value>>, DataError> {
    let [Value::Number(bytes)] = values else {
        return Err(DataError::BadArguments("runbook(bytes)".into()));
    };
    match *bytes {
        0. => Ok(None),
        1_048_576. | 4_194_304. => Ok(Some(vec![
            Value::str("paragraph"),
            Value::Number(*bytes),
            Value::Number(0.),
            Value::Number(0.),
            Value::Bool(true),
            Value::Bool(false),
        ])),
        _ => Err(DataError::BadArguments(
            "runbook bytes must be 0, 1048576 or 4194304".into(),
        )),
    }
}

fn project(value: Value) -> Value {
    let Value::Record(fields) = value else {
        unreachable!("existing Document record")
    };
    Value::record(vec![
        fields[0].clone(),
        fields[1].clone(),
        fields[9].clone(),
    ])
}
fn projected(answer: Answer) -> Answer {
    match answer {
        Answer::Now(value) => Answer::Now(project(value)),
        Answer::Later(request) => Answer::Later(request),
    }
}

impl Runbook {
    fn curated(&mut self) -> Value {
        // Drop the one old native source/cell; its running work stays owned by
        // the ordered executor until it actually exits. No UI join or history.
        self.stress = Source::default();
        self.curated.clone()
    }
    pub fn query(&mut self, values: &[Value]) -> Result<Value, DataError> {
        match args(values)? {
            None => Ok(self.curated()),
            Some(args) => self.stress.query("document", &args).map(project),
        }
    }
    pub fn answer(&mut self, store: &mut Store, values: &[Value]) -> Result<Answer, DataError> {
        match args(values)? {
            None => Ok(Answer::Now(self.curated())),
            Some(args) => self.stress.answer(store, "document", &args).map(projected),
        }
    }
    pub fn parse(
        &mut self,
        store: &mut Store,
        values: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        let Some(args) = args(values)? else {
            return Err(DataError::BadArguments(
                "curated runbook has no worker completion".into(),
            ));
        };
        self.stress
            .parse(store, "document", &args, outcome)
            .map(projected)
    }
    pub fn continuation(&mut self, token: u64) -> Option<Box<dyn FnOnce() -> Outcome + Send>> {
        self.stress.continuation(token)
    }
}
