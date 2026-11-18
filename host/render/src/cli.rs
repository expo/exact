//! The render entry's command line, [`main`]: an app's pages as documents for the web build and the parity check, or
//! the server.
use super::*;

/// An app's pages as documents, for the web build and the parity check:
///
/// `<app>-render [--plan <app.plan>] [--viewport <w>x<h>] [--name <name>]
/// [--origin <url>] [--deadline <ms>] [--shell <index.html>] (--build |
/// <location>…)`
///
/// renders each location with a fresh runner and executor at the page
/// viewport (the bake's 390 × 844 unless told) within the deadline (2 s
/// unless told), and prints one JSON line each: `location`, `notfound`,
/// `status` (503 when the deadline passed with requests in flight),
/// `settled`, `robots`, `root` (what `#exact-root` holds), `head` (what
/// `<head>` holds after the shell's charset and base), `checkpoint`,
/// `digest`, and with `--shell` the whole `page` ([`page`]), or `error`.
/// `--compare` renders each location with a kernel and without one (LLP
/// 1048.004 D6) and prints `same`, or the first difference (`differs`), or
/// why the page has no kernel-free render (`direct`) — with `--shell` the
/// streamed page too; any difference fails the run.
/// `--build` renders every location the plan declares `render=build`
/// (`exact_web::document::build_locations`) and every page a build route's
/// `pages=` source lists, and refuses a page whose render drew randomness:
/// its line holds an `error` naming its route instead (LLP 1048.000 D7).
/// `baked` is the app's own plan; the web build passes the one it extracted
/// from the shipped wasm instead. `data` makes the app's source: a fresh one
/// for each render (two per render, D6), for each enumeration, and for the
/// server's grants — so an entry is one call, `exact_render::main(PLAN, ||
/// …)`, with no wrapper to forward the source's methods.
///
/// `--serve <dist> [--port <n>] [--renders <n>] [--queue <n>] [--lifetime
/// <s>] [--generations <dir>]` is the server instead ([`Server`]): the built
/// web app on loopback, its plan the dist's own `app.plan` unless `--plan`
/// names one; `--generations` keeps builds at their versioned URLs and sends
/// deltas between them ([`Serve::generations`]).
/// It prints `serving http://127.0.0.1:<port>/`, then a line per render.
pub fn main<D: DataSource + 'static>(baked: &[u8], data: fn() -> D) -> std::process::ExitCode {
    main_with(
        baked,
        data,
        std::env::args().skip(1),
        &mut std::io::stdout(),
    )
}

/// [`main`] over `args`, the arguments after the program's name, writing its
/// JSON lines to `out`.
pub fn main_with<D: DataSource + 'static>(
    baked: &[u8],
    data: fn() -> D,
    args: impl IntoIterator<Item = String>,
    out: &mut impl std::io::Write,
) -> std::process::ExitCode {
    use std::process::ExitCode;
    let mut args = args.into_iter();
    let mut plan = baked.to_vec();
    let mut viewport = exact_runner::Viewport::default();
    let (mut name, mut origin) = (String::new(), None::<String>);
    let mut locations: Vec<(String, bool)> = Vec::new();
    let mut build = false;
    let mut compare = false;
    let mut deadline = DEADLINE;
    let (mut shell, mut rules) = (None::<String>, None::<String>);
    let (mut serve, mut planned) = (None::<std::path::PathBuf>, false);
    let mut generations = None::<std::path::PathBuf>;
    let mut plan_path = None::<std::path::PathBuf>;
    let (mut port, mut renders, mut queue, mut lifetime) = (0u16, 4usize, 32usize, 60u64);
    let usage = || {
        eprintln!("usage: render [--plan <app.plan>] [--viewport <w>x<h>] [--name <name>] [--origin <url>] [--deadline <ms>] ([--shell <index.html>] [--compare] (--build | <location>…) | --serve <dist> [--port <n>] [--renders <n>] [--queue <n>] [--lifetime <s>] [--generations <dir>])");
        ExitCode::from(2)
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--plan" => match args.next().map(|path| {
                plan_path = Some(path.clone().into());
                std::fs::read(path)
            }) {
                Some(Ok(bytes)) => {
                    plan = bytes;
                    planned = true;
                }
                Some(Err(e)) => {
                    eprintln!("render: --plan: {e}");
                    return ExitCode::FAILURE;
                }
                None => return usage(),
            },
            "--viewport" => {
                let size = args.next().and_then(|v| {
                    let (w, h) = v.split_once('x')?;
                    Some((w.parse().ok()?, h.parse().ok()?))
                });
                let Some((width, height)) = size else {
                    return usage();
                };
                viewport = exact_runner::Viewport::sized(width, height);
            }
            "--name" => match args.next() {
                Some(value) => name = value,
                None => return usage(),
            },
            "--origin" => match args.next() {
                Some(value) => origin = Some(value),
                None => return usage(),
            },
            "--deadline" => match args.next().and_then(|ms| ms.parse().ok()) {
                Some(ms) => deadline = Duration::from_millis(ms),
                None => return usage(),
            },
            "--shell" => match args.next().map(|path| {
                // The build's analyzed sheet beside it (LLP 1048.007 D4).
                let beside = std::path::Path::new(&path).with_file_name(".gen/page-style.json");
                rules = std::fs::read_to_string(beside).ok();
                std::fs::read_to_string(path)
            }) {
                Some(Ok(text)) => shell = Some(text),
                Some(Err(e)) => {
                    eprintln!("render: --shell: {e}");
                    return ExitCode::FAILURE;
                }
                None => return usage(),
            },
            "--build" => build = true,
            "--compare" => compare = true,
            "--serve" => match args.next() {
                Some(dist) => serve = Some(dist.into()),
                None => return usage(),
            },
            "--generations" => match args.next() {
                Some(dir) => generations = Some(dir.into()),
                None => return usage(),
            },
            "--port" | "--renders" | "--queue" | "--lifetime" => {
                let Some(n) = args.next().and_then(|n| n.parse::<u64>().ok()) else {
                    return usage();
                };
                match arg.as_str() {
                    "--port" => port = n.try_into().unwrap_or(0),
                    "--renders" => renders = n as usize,
                    "--queue" => queue = n as usize,
                    _ => lifetime = n,
                }
            }
            _ if arg.starts_with('/') => locations.push((arg, false)),
            _ => return usage(),
        }
    }
    if let (Some(dist), false) = (&serve, planned) {
        match std::fs::read(dist.join("app.plan")) {
            Ok(bytes) => plan = bytes,
            Err(e) => {
                eprintln!("render: {}/app.plan: {e}", dist.display());
                return ExitCode::FAILURE;
            }
        }
    }
    if let Some(dist) = &serve {
        plan_path = Some(dist.join("app.plan"));
    }
    let delivery = match plan_path.as_deref().map(delivery::beside).transpose() {
        Ok(delivery) => delivery.unwrap_or_default(),
        Err(e) => {
            eprintln!("render: version: {e}");
            return ExitCode::FAILURE;
        }
    };
    let decoded = match Plan::decode(&plan) {
        Ok(decoded) => decoded,
        Err(e) => {
            eprintln!("render: the plan: {e:?}");
            return ExitCode::FAILURE;
        }
    };
    if let Some(dist) = serve {
        let grants = exact_runner::grants::with_declared(data().grants(), &decoded);
        let config = Serve {
            dist,
            port,
            name,
            origin,
            deadline,
            renders,
            queue,
            viewport,
            lifetime: Duration::from_secs(lifetime),
            generations,
        };
        let server = match Server::bind(config, decoded, &grants) {
            Ok(server) => server,
            Err(e) => {
                eprintln!("render: serve: {e}");
                return ExitCode::FAILURE;
            }
        };
        return run_server(server, data);
    }
    // A build reports every refusal in its one run: it lists and renders every page, then fails
    let mut failed = false;
    if build {
        let (found, refused) = build_locations(&decoded);
        locations.extend(found);
        // The not-found document's clash is its location's line, as a refused page's is
        if let Some((location, error)) = refused {
            failed = true;
            writeln!(
                out,
                "{{\"location\":{},\"error\":{}}}",
                json(&location),
                json(&error)
            )
            .expect("render: the output");
        }
        // @ref LLP 1048.000 D2 — and every page a build route's source lists. A refused value's line is its error
        for row in decoded
            .routes
            .iter()
            .filter(|r| r.render == RenderPolicy::Build)
        {
            match pages(&decoded, data(), row, deadline) {
                Ok(listed) => {
                    locations.extend(listed.locations.into_iter().map(|l| (l, false)));
                    for (location, error) in listed.refused {
                        failed = true;
                        match location {
                            Some(location) => writeln!(
                                out,
                                "{{\"location\":{},\"error\":{}}}",
                                json(&location),
                                json(&error)
                            )
                            .expect("render: the output"),
                            None => eprintln!("render: {error}"),
                        }
                    }
                }
                Err(e) => {
                    eprintln!("render: {e}");
                    failed = true;
                }
            }
        }
    } else if locations.is_empty() {
        return usage();
    }
    let site = Site {
        name: &name,
        origin: origin.as_deref(),
    };
    for (location, listed) in &locations {
        let route = route_at(&decoded, location);
        // The not-found document, or any location the router sends there.
        let notfound = *listed || route.is_some_and(|r| r.notfound);
        let mut line = format!("{{\"location\":{}", json(location));
        if compare {
            let fields = compare::location(
                &decoded,
                &data,
                viewport,
                location,
                &site,
                deadline,
                shell.as_deref(),
                &delivery,
            );
            failed |= fields.starts_with(",\"same\":false");
            line.push_str(&fields);
            line.push('}');
            writeln!(out, "{line}").expect("render: the output");
            continue;
        }
        let ids = if shell.as_deref().is_some_and(page::is_js) {
            Ids::Any
        } else {
            Ids::Runtime
        };
        let rendered = render_with_at(
            &decoded,
            &data,
            viewport,
            location,
            &site,
            deadline,
            ids,
            Projection::Kernel,
            render_time(),
            &delivery,
        )
        .and_then(|rendered| {
            let page = shell
                .as_deref()
                .map(|shell| match &rules {
                    Some(rules) => page_with_rules(shell, rules, &rendered),
                    None => page(shell, &rendered),
                })
                .transpose()?;
            Ok((rendered, page))
        });
        match rendered {
            // @ref LLP 1048.000 D7 — refuse a page made at build whose render drew randomness, which every reader would share
            Ok((rendered, _)) if build && rendered.drew => {
                failed = true;
                let route = route.map_or("", |r| decoded.str(r.name));
                let error = format!("route `{route}` is render=build, but its render drew randomness, and a page made at build would give every reader the same random values (LLP 1069.005 D2). Declare the route render=request, rendered for each request, or render=client, rendered in the browser");
                let _ = write!(line, ",\"error\":{}", json(&error));
            }
            Ok((rendered, page)) => {
                let settled = rendered.settled == Settled::Complete;
                let status = rendered.status(notfound);
                let _ = write!(
                    line,
                    ",\"notfound\":{notfound},\"status\":{status},\"settled\":{settled},\"robots\":{}",
                    rendered
                        .document
                        .head
                        .robots
                        .as_deref()
                        .map_or_else(|| "null".into(), json)
                );
                for (field, value) in [
                    ("root", &rendered.document.root),
                    ("head", &rendered.head),
                    ("checkpoint", &rendered.checkpoint),
                    ("digest", &rendered.digest),
                ] {
                    let _ = write!(line, ",\"{field}\":{}", json(value));
                }
                if let Some(page) = &page {
                    let _ = write!(line, ",\"page\":{}", json(page));
                }
            }
            Err(error) => {
                failed = true;
                let _ = write!(line, ",\"error\":{}", json(&error));
            }
        }
        line.push('}');
        writeln!(out, "{line}").expect("render: the output");
    }
    shutdown_render_thread();
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
