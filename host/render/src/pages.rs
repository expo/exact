//! Enumeration (LLP 1048.000 D2): a parameterized route's pages, from the
//! source its `pages=` names — asked once, in a render's environment, each
//! value of the route's one parameter made a location by the route table.
//! The build renders them; the server lists them in its sitemap.
use crate::{Anonymous, Executor};
use exact_plan::{Plan, RoutesRow, Value};
use exact_runner::{Answer, DataSource, Dispatch, RequestOut, Store};
use std::time::{Duration, Instant};

/// A route's listed pages: the locations that are its own, and each value
/// that is no page of the route, with why it is refused.
#[derive(Debug, Default, PartialEq)]
pub struct Listed {
    /// The route's own pages.
    pub locations: Vec<String>,
    /// `(location, why)`: a value the route's pattern can't hold has no
    /// location, and one whose location the router gives another route is
    /// that route's page (LLP 1048.000 D2).
    pub refused: Vec<(Option<String>, String)>,
}

/// The pages route `row` lists, or none when it names no source; `Err` when
/// its source fails or its answer is no list of strings.
pub fn pages<D: DataSource>(
    plan: &Plan,
    data: D,
    row: &RoutesRow,
    deadline: Duration,
) -> Result<Listed, String> {
    let source = plan.str(row.pages);
    if source.is_empty() {
        return Ok(Listed::default());
    }
    let route = plan.str(row.name);
    let fail = |why: String| format!("route `{route}`'s pages ({source}): {why}");
    let args = match Value::from_bytes(plan.bytes(row.pages_args)) {
        Ok(Value::List(args)) => args.to_vec(),
        _ => return Err(fail("its arguments don't decode".into())),
    };
    let mut data = Anonymous::new(data);
    // The deadline waits for the source, not for the transport to start.
    let executor = Executor::start(data.grants());
    let until = Instant::now() + deadline;
    data.bind(plan);
    loop {
        match data.preload() {
            Ok(true) => break,
            Ok(false) if Instant::now() < until => std::thread::sleep(Duration::from_millis(1)),
            Ok(false) => return Err(fail("the data module didn't prepare in time".into())),
            Err(e) => return Err(fail(format!("{e:?}"))),
        }
    }
    data.activate().map_err(|e| fail(format!("{e:?}")))?;
    let mut store = Store::new(data.grants(), Vec::new());
    let mut answer = data
        .answer(&mut store, source, &args)
        .map_err(|e| fail(format!("{e:?}")))?;
    let mut ticket = 0;
    let value = loop {
        let request = match answer {
            Answer::Now(value) => break value,
            Answer::Later(request) => request,
        };
        // Every turn meets the deadline: a source re-asked without end gets
        // its outcome at once, every time.
        if Instant::now() >= until {
            return Err(fail("it didn't answer before the deadline".into()));
        }
        ticket += 1;
        let out = RequestOut {
            completion: None,
            class: exact_runner::RequestClass::Ordered,
            ticket,
            target: format!("{route} pages"),
            request,
            forced: false,
        };
        // A long poll on work that is no HTTP request (LLP 1016.000 D8): refused before its work runs.
        let dispatch = match out.request.continuation {
            _ if out.request.long_poll_refusal().is_some() => Dispatch::Missing,
            Some(token) => data.dispatch(token, &store),
            None => Dispatch::Missing,
        };
        match dispatch {
            Dispatch::Run(work) => executor.run(out, Some(work)),
            Dispatch::Held => Err("its work was held"),
            // A source asks its answer again: it settles with no work.
            Dispatch::Again => executor.again(&out),
            Dispatch::Host(_) | Dispatch::Missing => executor.run(out, None),
        }
        .map_err(|e| fail(e.into()))?;
        let outcome = loop {
            executor.begin_pump();
            if let Some((_, outcome, _)) = executor.drain().pop() {
                break outcome;
            }
            if Instant::now() >= until {
                return Err(fail("it didn't answer before the deadline".into()));
            }
            executor.wait(until);
        };
        answer = data
            .parse(&mut store, source, &args, outcome)
            .map_err(|e| fail(format!("{e:?}")))?;
    };
    let Value::List(values) = value else {
        return Err(fail("its answer isn't a list".into()));
    };
    let values = values
        .iter()
        .map(|value| value.as_str())
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| fail("its answer isn't a list of strings".into()))?;
    let mut listed = Listed::default();
    for value in values {
        let location = match exact_web::document::route_location(plan, route, value) {
            Ok(location) => location,
            Err(why) => {
                let value = serde_json::to_string(value).unwrap();
                listed.refused.push((None, fail(format!("the value {value} can't be a path segment: {why}. Leave the value out of the list"))));
                continue;
            }
        };
        // The router gives the location to the first declared pattern that matches it, and the route's own matches it,
        // so the route that answers is this one or one declared before it
        let other = exact_web::document::route_at(plan, &location).map_or("", |r| plan.str(r.name));
        if other == route {
            listed.locations.push(location);
        } else {
            let value = serde_json::to_string(value).unwrap();
            let why = fail(format!("the value {value} is at {location}, which route `{other}`, declared before `{route}`, answers, so the page there is `{other}`'s, not `{route}`'s. Leave the value out of the list, or give one of the two routes another path"));
            listed.refused.push((Some(location), why));
        }
    }
    Ok(listed)
}
