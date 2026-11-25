use exact_runner::{Answer, DataError, DataSource, Outcome, Request, Store, Value};

#[allow(dead_code, clippy::all)]
mod shapes {
    include!(concat!(env!("OUT_DIR"), "/shapes.rs"));
}
use shapes::{ContractValue, Section};

#[derive(Default)]
pub struct ProgressiveTabs;

impl DataSource for ProgressiveTabs {
    fn grants(&self) -> &str {
        "net.fetch http://127.0.0.1:4789"
    }

    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }

    fn answer(&mut self, _: &mut Store, source: &str, args: &[Value]) -> Result<Answer, DataError> {
        let filter = args
            .first()
            .and_then(Value::as_str)
            .ok_or_else(|| DataError::BadArguments(source.into()))?;
        let section = match source {
            "loadRows" => "rows",
            "loadHistory" => "history",
            _ => return Err(DataError::UnknownSource(source.into())),
        };
        let request = Request::get(&format!("http://127.0.0.1:4789/{section}/{filter}"));
        let request = if section == "history" && args.get(1) == Some(&Value::Bool(false)) {
            request.independent_http(4096)
        } else {
            request
        };
        Ok(Answer::Later(request))
    }

    fn parse(
        &mut self,
        _: &mut Store,
        source: &str,
        args: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        let response = match outcome {
            Outcome::Response(r) if r.status == 200 => r,
            Outcome::Response(r) => {
                return Err(DataError::Unavailable(format!("HTTP {}", r.status)))
            }
            _ => return Err(DataError::Unavailable(format!("{source} failed"))),
        };
        let body = std::str::from_utf8(&response.body)
            .map_err(|_| DataError::Unavailable("Invalid text".into()))?;
        let filter = args
            .first()
            .and_then(Value::as_str)
            .ok_or_else(|| DataError::BadArguments(source.into()))?;
        Ok(Answer::Now(
            Section {
                filter: filter.into(),
                items: body.lines().map(str::to_owned).collect(),
            }
            .to_value(),
        ))
    }
}
