//! The compiler's time grows with the plan, not its square: a root holding
//! four times the uses of one component compiles in about four times as long.
//! Each per-use pass that answered a question by scanning or copying every
//! declaration made that longer: such scans once took 8,000 uses to ten
//! seconds, and main measured 17.1x here before they went. Put back one at a
//! time, each of the thirteen takes this test's ratio from 3.9-4.3 to 4.8-16.
// Thread CPU time is read through `libc`, a dependency on unix only.
#![cfg(unix)]
use std::path::Path;
use std::time::Duration;
#[path = "it/scratch.rs"]
#[allow(dead_code)]
mod scratch;
use scratch::Scratch;

/// A component's states, actions and buttons, one of each per `j`.
const EACH: usize = 16;
/// Root states per use, each with an initializer that reads another state.
const ROOTS: usize = 4;

/// `uses` uses of a component with a prop and [`EACH`] states, actions and
/// buttons pressing them, each action calling one more whose parameter the
/// calls type, inlined into the root, beside [`ROOTS`] root states a use. The
/// buttons are not the platform's own, so the warning pass reads their faces'
/// rows at every use. Every per-use question the compiler asks (a slot's type
/// after a body or a call, a name in scope, a state or action by name, a
/// button's face) is asked many times a use, so a scan for any of them shows.
fn app(uses: usize) -> String {
    let mut source = String::from("component App\n  state first = 0\n");
    for i in 0..uses * ROOTS {
        source.push_str(&format!("  state s{i} = first\n"));
    }
    source.push_str("  view\n    column\n");
    for i in 0..uses {
        source.push_str(&format!("      Card(label=\"c{i}\")\n"));
    }
    source.push_str("\ncomponent Card\n  props\n    label: string\n");
    for j in 0..EACH {
        source.push_str(&format!("  state n{j} = 0\n"));
    }
    source.push_str("  state total = 0\n  action add(by)\n    total = total + by\n");
    for j in 0..EACH {
        source.push_str(&format!(
            "  action bump{j}\n    n{j} = n{j} + 1\n    add(1)\n"
        ));
    }
    source.push_str("  view\n    column\n      text label\n");
    for j in 0..EACH {
        source.push_str(&format!(
            "      button press=bump{j} appearance=\"none\"\n        text `${{n{j}}}`\n"
        ));
    }
    source
}

/// This thread's CPU time: what a busy machine's other work does not add to.
fn cpu() -> Duration {
    let mut t = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `t` is a valid timespec for the call to fill.
    assert_eq!(
        unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut t) },
        0
    );
    Duration::new(t.tv_sec as u64, t.tv_nsec as u32)
}

/// The mapped compilation with its warnings, excluding the CLI's separate
/// advice passes: its CPU time and its plan's bytes.
fn timed(path: &Path) -> (Duration, Vec<u8>) {
    let start = cpu();
    let (compiled, _warnings) = contract::compile_path_all_warned(path, true);
    let took = cpu() - start;
    (took, compiled.unwrap().0.encode())
}

#[test]
fn four_times_the_uses_compile_in_about_four_times_as_long() {
    let uses = 250;
    let dir = Scratch::new(format!("contract-growth-{}", std::process::id()));
    let (small_app, large_app) = (dir.join("small.contract"), dir.join("large.contract"));
    std::fs::write(&small_app, app(uses)).unwrap();
    std::fs::write(&large_app, app(uses * 4)).unwrap();
    // The least CPU time of three of each, taken in turns.
    let (mut small, mut large) = (Duration::MAX, Duration::MAX);
    let (mut small_bytes, mut large_bytes) = (None, None);
    for _ in 0..3 {
        for (app, best, bytes) in [
            (&small_app, &mut small, &mut small_bytes),
            (&large_app, &mut large, &mut large_bytes),
        ] {
            let (took, plan) = timed(app);
            *best = (*best).min(took);
            assert!(
                bytes.as_ref().is_none_or(|b| *b == plan),
                "the same source compiled to different bytes"
            );
            *bytes = Some(plan);
        }
    }
    std::fs::remove_dir_all(&dir).unwrap();
    let ratio = large.as_secs_f64() / small.as_secs_f64();
    // Linear measures 3.9-4.3 (debug and host-dev); the least of the scans
    // put back, a handler's action found by `position`, measures 4.8-5.2.
    assert!(
        ratio <= 4.6,
        "{uses} uses took {small:?} and {} took {large:?}: {ratio:.1}x for 4x the uses",
        uses * 4
    );
}

// The CLI runs clock advice outside the compilation timed above.
fn clock_advice_growth(declarations: usize, source: impl Fn(usize) -> String) {
    let small_file = contract_syntax::parse(&source(declarations)).unwrap();
    let large_file = contract_syntax::parse(&source(declarations * 4)).unwrap();
    let (mut small, mut large) = (Duration::MAX, Duration::MAX);
    for _ in 0..3 {
        for (file, best) in [(&small_file, &mut small), (&large_file, &mut large)] {
            let start = cpu();
            let warnings = contract_analyze::resource_clock_warnings(file);
            *best = (*best).min(cpu() - start);
            assert!(warnings.is_empty(), "{warnings:?}");
        }
    }
    let ratio = large.as_secs_f64() / small.as_secs_f64();
    eprintln!(
        "clock advice: {declarations} declarations {small:?}, {} {large:?}: {ratio:.1}x",
        declarations * 4
    );
    // Leave room for ordered-map lookups, but reject quadratic growth.
    assert!(
        ratio <= 6.0,
        "clock advice grew {ratio:.1}x for 4x the declarations"
    );
}

#[test]
fn resource_clock_advice_grows_with_functions() {
    clock_advice_growth(50, |count| {
        let mut source = String::new();
        for i in 0..count {
            source.push_str(&format!("fn f{i}(x: number): number = x + {i}\n"));
        }
        source.push_str("component App\n  state s = 0\n  view\n    text `${s}`\n");
        source
    });
}

#[test]
fn resource_clock_advice_grows_with_derives() {
    clock_advice_growth(300, |count| {
        let mut source = String::from("component App\n  state s = 0\n");
        for i in 0..count {
            source.push_str(&format!("  derive d{i} = s + {i}\n"));
        }
        source.push_str("  view\n    text `${s}`\n");
        source
    });
}

#[test]
fn imported_names_grow_with_the_scope() {
    let dir = Scratch::new(format!("contract-import-growth-{}", std::process::id()));
    let mut paths = Vec::new();
    for count in [500, 2000] {
        let shapes = (0..count)
            .map(|i| format!("shape S{i}\n  id: string\n"))
            .collect::<String>();
        let names = (0..count).map(|i| format!("S{i}")).collect::<Vec<_>>();
        std::fs::write(dir.join(format!("shapes{count}.contract")), shapes).unwrap();
        let path = dir.join(format!("app{count}.contract"));
        std::fs::write(&path, format!(
            "use {} from \"./shapes{count}.contract\"\ncomponent App\n  state s: option<S0> = none\n  view\n    text \"imports\"\n",
            names.join(", ")
        )).unwrap();
        paths.push(path);
    }
    scope_growth(&dir, &paths);
}

fn scope_growth(dir: &Path, paths: &[std::path::PathBuf]) {
    let mut best = [Duration::MAX; 2];
    for _ in 0..3 {
        for (i, path) in paths.iter().enumerate() {
            best[i] = best[i].min(timed(path).0);
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
    let ratio = best[1].as_secs_f64() / best[0].as_secs_f64();
    eprintln!("scopes: {best:?}, {ratio:.2}x");
    assert!(
        ratio < 7.0,
        "4x the declarations took {ratio:.2}x the CPU time"
    );
}

#[test]
fn file_scopes_share_the_programs_names() {
    let dir = Scratch::new(format!("contract-files-growth-{}", std::process::id()));
    let mut paths = Vec::new();
    for count in [10, 40] {
        let mut source = String::new();
        for i in 0..count {
            source.push_str(&format!("use S{i}_0 from \"./s{i}.contract\"\n"));
            let shapes = (0..50)
                .map(|j| format!("shape S{i}_{j}\n  id: string\n"))
                .collect::<String>();
            std::fs::write(dir.join(format!("s{i}.contract")), shapes).unwrap();
        }
        source.push_str(
            "component App\n  state s: option<S0_0> = none\n  view\n    text \"files\"\n",
        );
        let path = dir.join(format!("app{count}.contract"));
        std::fs::write(&path, source).unwrap();
        paths.push(path);
    }
    scope_growth(&dir, &paths);
}

#[test]
fn action_injects_grow_with_the_channels_they_reach() {
    let dir = Scratch::new(format!("contract-inject-growth-{}", std::process::id()));
    let mut paths = Vec::new();
    for count in [200, 800] {
        let mut source = String::from("component Root\n  state n = 0\n");
        for i in 0..count {
            source.push_str(&format!("  action a{i}\n    n = n + {i}\n"));
        }
        source.push_str("  provide\n");
        for i in 0..count {
            source.push_str(&format!("    a{i}\n"));
        }
        source.push_str("  view\n    column\n");
        for i in 0..count / 10 {
            source.push_str(&format!("      Child{i}()\n"));
        }
        for i in 0..count / 10 {
            source.push_str(&format!("component Child{i}\n  inject\n"));
            for j in 0..40 {
                source.push_str(&format!("    a{}: action\n", (i * 7 + j) % count));
            }
            source.push_str(&format!(
                "  view\n    button press=a{}\n      text \"child\"\n",
                i * 7 % count
            ));
        }
        let path = dir.join(format!("app{count}.contract"));
        std::fs::write(&path, source).unwrap();
        paths.push(path);
    }
    scope_growth(&dir, &paths);
}

#[test]
fn parsing_providers_grows_with_the_section() {
    let source = |count| {
        let mut source = String::from("component App\n  state n = 0\n  provide\n");
        for i in 0..count {
            source.push_str(&format!("    p{i} = n\n"));
        }
        source.push_str("  view\n    text \"providers\"\n");
        source
    };
    let sources = [source(1000), source(4000)];
    let mut best = [Duration::MAX; 2];
    for _ in 0..3 {
        for (i, source) in sources.iter().enumerate() {
            let start = cpu();
            let file = contract_syntax::parse(source).unwrap();
            best[i] = best[i].min(cpu() - start);
            assert_eq!(
                file.components[0].provides.len(),
                if i == 0 { 1000 } else { 4000 }
            );
        }
    }
    let ratio = best[1].as_secs_f64() / best[0].as_secs_f64();
    eprintln!("provide parsing: {best:?}, {ratio:.2}x");
    assert!(
        ratio < 7.0,
        "4x the providers took {ratio:.2}x the CPU time"
    );
}
