//! Sources declared in Contract (LLP 1162.001): the runner writes the
//! request from the plan's row and the call's arguments, the host runs it as
//! it runs any source's, and the runner reads the reply into the shape. No
//! data module is asked.

use super::*;
use crate::failure::FailureCode;
use crate::fetch_template::{self as template, Part};
use crate::request::{Answer, Outcome, Response};
use exact_js_value::Shape;
use exact_plan::TypesId;
use serde_json::Value as Json;

fn failed(code: FailureCode, message: String) -> DataError {
    DataError::Failed(code, message, String::new())
}

impl<D: DataSource> Runner<D> {
    /// Whether `source` can be asked now: the data module's once it is
    /// ready, a declared source always, since no module answers it.
    pub(super) fn source_ready(&self, source: &str) -> bool {
        self.data.ready() || self.plan.fetch_named(source).is_some()
    }

    /// The request declared source `source` makes for `args` (D1): a
    /// resource's is a read, a send's a write (D4).
    pub(super) fn fetch_answer(
        &mut self,
        source: &str,
        args: &[Value],
    ) -> Result<Answer, DataError> {
        let plan = &self.plan;
        let row = plan.fetch_named(source).expect("a declared source").clone();
        let names: Vec<&str> = plan.fetch_parts(row.params).map(|(n, _)| n).collect();
        let url = template::expand(plan.str(row.url), |part| match part {
            Part::Param(name) => {
                template::argument(&names, args, name).map(|t| template::encode(&t))
            }
            Part::Secret(_) => None,
        })
        .ok_or_else(|| {
            DataError::BadArguments(format!("{source}: an argument its URL names is not text"))
        })?;
        let mut request = Request::get(&url);
        request.method = plan.str(row.method).to_string();
        // Its own origin is its scope: a redirect elsewhere is refused as
        // for any request (D3).
        request.grants = Some(format!("net.fetch {}", plan.str(row.origin)));
        let store = &self.store;
        for (name, value) in plan.fetch_parts(row.headers) {
            let mut absent = None;
            let filled = template::expand(value, |part| match part {
                Part::Param(n) => template::argument(&names, args, n),
                Part::Secret(secret) => match store.read(secret) {
                    Ok(Some(v)) => Some(v.to_string()),
                    _ => {
                        absent = Some(secret.to_string());
                        None
                    }
                },
            });
            match filled {
                Some(v) if !v.contains(['\r', '\n', '\0']) => {
                    request.headers.push((name.to_string(), v))
                }
                Some(_) => {
                    return Err(failed(
                        FailureCode::Error,
                        format!("{source}: the header `{name}` would have a line break"),
                    ))
                }
                None => {
                    let secret = absent.unwrap_or_default();
                    return Err(failed(
                        FailureCode::Refused,
                        format!("{source}: the kept secret `{secret}` its header `{name}` names is absent or not granted, so nothing was sent"),
                    ));
                }
            }
        }
        let has = |request: &Request, header: &str| {
            (request.headers.iter()).any(|(n, _)| n.eq_ignore_ascii_case(header))
        };
        if !has(&request, "accept") {
            request
                .headers
                .push(("Accept".into(), "application/json".into()));
        }
        let body = plan.str(row.body);
        if !body.is_empty() {
            let signature = &plan.sources[row.source.0 as usize];
            let mut variables = serde_json::Map::new();
            for (i, name) in names.iter().enumerate() {
                let ty = plan.source_params[(signature.params.start as usize) + i].ty;
                let shape = Shape::from_plan(plan, ty).map_err(DataError::Unavailable)?;
                let value = args.get(i).ok_or_else(|| {
                    DataError::BadArguments(format!("{source}: argument {} is missing", i + 1))
                })?;
                let json = exact_js_value::to_json(value, &shape)
                    .map_err(|e| DataError::BadArguments(format!("{source}: `{name}`: {e}")))?;
                variables.insert(name.to_string(), json);
            }
            let json = match body {
                "graphql" => serde_json::json!({
                    "query": plan.str(row.document),
                    "variables": variables,
                }),
                _ => Json::Object(variables),
            };
            request.body = json.to_string().into_bytes();
            if !has(&request, "content-type") {
                request
                    .headers
                    .push(("Content-Type".into(), "application/json".into()));
            }
        }
        Ok(Answer::Later(request))
    }

    /// The reply to declared source `source`'s request, read into `ty` (D2),
    /// or the failure a TypeScript source's would be (D5).
    pub(super) fn fetch_parse(
        &mut self,
        ty: TypesId,
        source: &str,
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        let row = self
            .plan
            .fetch_named(source)
            .expect("a declared source")
            .clone();
        let body = match outcome {
            Outcome::Response(Response { status, body, .. }) if (200..300).contains(&status) => {
                body
            }
            Outcome::Response(Response { status, .. }) => {
                return Err(failed(
                    FailureCode::Error,
                    format!("{source}: HTTP {status} from {}", self.plan.str(row.origin)),
                ))
            }
            Outcome::Failed { kind, message } => {
                return Err(failed(FailureCode::of_kind(kind), message))
            }
            _ => return Err(failed(FailureCode::Error, format!("{source}: no response"))),
        };
        let json: Json = serde_json::from_slice(&body).map_err(|e| {
            failed(
                FailureCode::Error,
                format!("{source}: the reply is not JSON: {e}"),
            )
        })?;
        if self.plan.str(row.body) == "graphql" {
            if let Some((message, code)) = template::graphql_error(&json) {
                return Err(DataError::Failed(
                    FailureCode::Error,
                    format!("{source}: {message}"),
                    code,
                ));
            }
        }
        let pointer = self.plan.str(row.pointer);
        let shaped = |why: String| failed(FailureCode::Shape, format!("{source}: {why}"));
        let at = template::pointer(&json, pointer)
            .ok_or_else(|| shaped(format!("the reply has nothing at `{pointer}`")))?;
        let shape = Shape::from_plan(&self.plan, ty).map_err(DataError::Unavailable)?;
        let value = exact_js_value::project_json(at, &shape).map_err(&shaped)?;
        if !value.conforms(&self.plan, ty) {
            return Err(shaped(crate::conform::mismatch(&self.plan, &value, ty)));
        }
        Ok(Answer::Now(value))
    }
}
