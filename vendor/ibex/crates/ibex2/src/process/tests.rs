use super::*;
use std::io::{Read, Write};
#[cfg(target_vendor = "apple")]
use std::os::fd::AsRawFd;

fn sh(script: &str, stdin: Stdio) -> Child {
    let args = vec!["-c".to_string(), script.to_string()];
    let env = vec![
        ("PATH".to_string(), "/usr/bin:/bin".to_string()),
        ("GREETING".to_string(), "hi".to_string()),
    ];
    spawn(Command {
        arg0: None,
        program: Path::new("/bin/sh"),
        args: &args,
        env: &env,
        cwd: Path::new("/"),
        stdin,
        stdout: Stdio::Pipe,
        stderr: Stdio::Pipe,
        session: false,
    })
    .expect("sh starts")
}

fn read_all(fd: OwnedFd) -> String {
    let mut out = String::new();
    std::fs::File::from(fd).read_to_string(&mut out).unwrap();
    out
}

#[cfg(target_os = "linux")]
#[test]
fn a_zombie_thread_group_leader_does_not_hide_its_live_worker() {
    const CHILD: &str = "EXACT_TEST_ZOMBIE_WORKER";
    if std::env::var_os(CHILD).is_some() {
        extern "C" fn exit_main_thread(_: libc::c_int) {
            // SAFETY: SYS_exit ends only the calling thread, as the last
            // step of pthread_exit does. The test thread stays alive.
            unsafe { libc::syscall(libc::SYS_exit, 0) };
        }
        // SAFETY: this isolated child installs its own signal dispositions
        // and directs the exit handler to its main thread alone.
        unsafe {
            libc::signal(libc::SIGTERM, libc::SIG_IGN);
            libc::signal(
                libc::SIGUSR1,
                exit_main_thread as *const () as libc::sighandler_t,
            );
            libc::syscall(
                libc::SYS_tgkill,
                libc::getpid(),
                libc::getpid(),
                libc::SIGUSR1,
            );
        }
        println!("worker-ready");
        std::io::stdout().flush().unwrap();
        loop {
            std::thread::park();
        }
    }
    let child = spawn(Command {
        arg0: None,
        program: &std::env::current_exe().unwrap(),
        args: &[
            "--exact".into(),
            "process::tests::a_zombie_thread_group_leader_does_not_hide_its_live_worker".into(),
            "--nocapture".into(),
        ],
        env: &[(CHILD.into(), "1".into())],
        cwd: Path::new("/"),
        stdin: Stdio::Null,
        stdout: Stdio::Pipe,
        stderr: Stdio::Null,
        session: false,
    })
    .unwrap();
    let mut stdout = std::io::BufReader::new(std::fs::File::from(child.stdout.unwrap()));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut line = String::new();
    while !line.contains("worker-ready") {
        use std::io::BufRead;
        assert!(stdout.read_line(&mut line).unwrap() > 0, "{line}");
    }
    let zombie = loop {
        let stat = std::fs::read_to_string(format!("/proc/{}/stat", child.pid)).unwrap();
        if stat[stat.rfind(')').unwrap() + 1..]
            .trim_start()
            .starts_with('Z')
        {
            break true;
        }
        if std::time::Instant::now() >= deadline {
            break false;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    let alive = group_alive(child.pid);
    let members = group_members(child.pid);
    signal_group(child.pid, SIGTERM).unwrap();
    signal_group(child.pid, SIGKILL).unwrap();
    let exit = wait(child.pid).unwrap();
    assert!(zombie, "the main thread never exited");
    assert!(alive, "the worker still needs escalation");
    assert!(members.contains(&child.pid), "{members:?}");
    assert_eq!(exit, Exit::Signal(SIGKILL));
}

#[cfg(target_os = "linux")]
#[test]
fn a_group_is_conservatively_alive_when_proc_cannot_be_read() {
    const CHILD: &str = "EXACT_TEST_NO_PROCESS_FDS";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "process::tests::a_group_is_conservatively_alive_when_proc_cannot_be_read",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        return;
    }
    let child = sh("read line", Stdio::Pipe);
    let limit = libc::rlimit {
        rlim_cur: 64,
        rlim_max: 64,
    };
    // SAFETY: only this isolated subprocess's descriptor limit changes.
    assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &limit) }, 0);
    let mut files = Vec::new();
    while let Ok(file) = std::fs::File::open("/dev/null") {
        files.push(file);
    }
    let error = std::fs::read_dir("/proc").unwrap_err();
    let alive = group_alive(child.pid);
    signal_group(child.pid, SIGKILL).unwrap();
    let exit = wait(child.pid).unwrap();
    drop(files);
    assert_eq!(error.raw_os_error(), Some(libc::EMFILE));
    assert!(alive, "inspection failure does not prove the group ended");
    assert_eq!(exit, Exit::Signal(SIGKILL));
}

#[cfg(target_os = "linux")]
#[test]
fn a_non_utf8_process_name_does_not_keep_an_unrelated_group_alive() {
    let mut unrelated = sh(
        "printf 'invalid\\377) (name' > /proc/$$/comm; echo ready; read line; exit 0",
        Stdio::Pipe,
    );
    let mut line = String::new();
    {
        use std::io::BufRead;
        std::io::BufReader::new(std::fs::File::from(unrelated.stdout.take().unwrap()))
            .read_line(&mut line)
            .unwrap();
    }
    let stat = std::fs::read(format!("/proc/{}/stat", unrelated.pid)).unwrap();
    let child = spawn(Command {
        arg0: None,
        program: Path::new("/bin/true"),
        args: &[],
        env: &[],
        cwd: Path::new("/"),
        stdin: Stdio::Null,
        stdout: Stdio::Null,
        stderr: Stdio::Null,
        session: false,
    })
    .unwrap();
    let observed = observe_exit(child.pid).unwrap();
    let alive = group_alive(child.pid);
    let members = group_members(unrelated.pid);
    let started = start_time(unrelated.pid);
    let reaped = wait(child.pid).unwrap();
    drop(unrelated.stdin.take());
    let unrelated_exit = wait(unrelated.pid).unwrap();

    assert_eq!(line, "ready\n");
    assert!(
        stat.contains(&0xff),
        "the process name must contain invalid UTF-8"
    );
    assert!(
        !alive,
        "an unrelated process cannot keep an exited group alive"
    );
    assert_eq!(members, [unrelated.pid]);
    assert!(started.is_some(), "the name does not affect the start time");
    assert_eq!(observed, Exit::Code(0));
    assert_eq!(reaped, observed);
    assert_eq!(unrelated_exit, Exit::Code(0));
}

#[test]
fn observing_exit_reserves_the_pid_without_keeping_the_group_alive() {
    for (script, expected) in [
        ("exit 7", Exit::Code(7)),
        ("kill -TERM $$", Exit::Signal(libc::SIGTERM)),
    ] {
        let child = sh(script, Stdio::Null);
        assert_eq!(observe_exit(child.pid).unwrap(), expected);
        let alive = group_alive(child.pid);
        let members = group_members(child.pid);
        // A signal cannot reach another group while the zombie holds the id.
        let signalled = signal_group(child.pid, SIGKILL);
        let observed_again = observe_exit(child.pid);
        let reaped = wait(child.pid);
        assert!(!alive, "a zombie needs no escalation");
        assert!(members.is_empty(), "{members:?}");
        // Linux signals a zombie (to no effect); XNU refuses with EPERM.
        assert!(
            signalled.is_ok()
                || signalled
                    .as_ref()
                    .is_err_and(|e| matches!(e.raw_os_error(), Some(libc::ESRCH | libc::EPERM))),
            "{signalled:?}"
        );
        assert_eq!(observed_again.unwrap(), expected);
        assert_eq!(reaped.unwrap(), expected);
        assert_eq!(
            wait(child.pid).unwrap_err().raw_os_error(),
            Some(libc::ECHILD)
        );
    }
}

#[test]
fn a_process_is_signalled_only_while_its_test_holds() {
    let child = sh("read line", Stdio::Pipe);
    let refused = signal_process_if(child.pid, SIGKILL, |_| false).unwrap();
    let running = try_observe_exit(child.pid).unwrap();
    let mut tested = None;
    let sent = signal_process_if(child.pid, SIGKILL, |pid| {
        tested = Some(pid);
        true
    })
    .unwrap();
    let exit = wait(child.pid).unwrap();
    assert!(!refused);
    assert_eq!(running, None, "a refused test sent the signal");
    assert!(sent);
    assert_eq!(tested, Some(child.pid));
    assert_eq!(exit, Exit::Signal(SIGKILL));
}

#[test]
fn a_process_reaped_during_its_test_is_not_signalled() {
    let child = sh("read line", Stdio::Pipe);
    let sent = signal_process_if(child.pid, SIGTERM, |pid| {
        signal_group(pid, SIGKILL).unwrap();
        wait(pid).unwrap();
        true
    });
    // On Linux the pidfd still names the reaped process, whatever holds its pid now.
    assert_eq!(sent.unwrap_err().raw_os_error(), Some(libc::ESRCH));
}

#[test]
fn a_child_has_its_own_group_its_environment_its_directory_and_only_three_descriptors() {
    // A descriptor this process holds without close-on-exec, which no child may inherit.
    // SAFETY: a NUL-terminated path; the descriptor is closed below.
    let open = unsafe { libc::open(c"/dev/null".as_ptr(), libc::O_RDONLY) };
    let mut child = sh(&format!("echo $GREETING; pwd; ps -o pgid= -p $$ | tr -d ' '; [ -e /dev/fd/{open} ] && echo leaked || echo closed; echo err >&2"), Stdio::Pipe);
    // SAFETY: opened above.
    unsafe { libc::close(open) };
    drop(child.stdin.take());
    let out = read_all(child.stdout.take().unwrap());
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "hi");
    assert_eq!(lines[1], "/");
    assert_eq!(lines[2], child.pid.to_string(), "its own process group");
    assert_eq!(lines[3], "closed", "only 0, 1 and 2 are inherited");
    assert_eq!(read_all(child.stderr.take().unwrap()), "err\n");
    assert_eq!(wait(child.pid).unwrap(), Exit::Code(0));
}

#[test]
fn stdin_is_the_childs_and_a_signal_to_the_group_reaches_its_children() {
    let mut child = sh(
        "read line; echo \"got $line\"; sleep 30 & echo $!; wait",
        Stdio::Pipe,
    );
    let mut stdin = std::fs::File::from(child.stdin.take().unwrap());
    stdin.write_all(b"ping\n").unwrap();
    drop(stdin);
    let mut stdout = std::fs::File::from(child.stdout.take().unwrap());
    let mut seen = String::new();
    let mut byte = [0u8; 1];
    while seen.matches('\n').count() < 2 {
        stdout.read_exact(&mut byte).unwrap();
        seen.push(byte[0] as char);
    }
    let mut lines = seen.lines();
    assert_eq!(lines.next(), Some("got ping"));
    let grandchild: i32 = lines.next().unwrap().parse().unwrap();
    assert!(
        group_members(child.pid).contains(&grandchild),
        "the grandchild is in the group"
    );
    assert!(start_time(grandchild).unwrap() >= start_time(child.pid).unwrap());
    signal_group(child.pid, libc::SIGTERM).unwrap();
    assert_eq!(wait(child.pid).unwrap(), Exit::Signal(libc::SIGTERM));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while group_alive(child.pid) {
        assert!(
            std::time::Instant::now() < deadline,
            "the group outlived its signal"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[cfg(target_vendor = "apple")]
#[test]
fn group_listing_uses_libprocs_pid_count_without_a_terminator() {
    let mut child = sh("read line; exit 0", Stdio::Pipe);
    let mut pids = [-7 as libc::pid_t; 4];
    // SAFETY: null queries the count; the second buffer holds its stated byte size.
    let (estimate, n) = unsafe {
        (
            libc::proc_listpgrppids(child.pid, std::ptr::null_mut(), 0),
            libc::proc_listpgrppids(
                child.pid,
                pids.as_mut_ptr().cast(),
                std::mem::size_of_val(&pids) as libc::c_int,
            ),
        )
    };
    let members = group_members(child.pid);
    drop(child.stdin.take());
    let exit = wait(child.pid).unwrap();
    let gone = group_members(child.pid);

    assert!(estimate >= 1);
    assert_eq!(n, 1, "libproc returns a PID count, not a byte count");
    assert_eq!(pids, [child.pid, -7, -7, -7]);
    assert_eq!(members, [child.pid]);
    assert_eq!(exit, Exit::Code(0));
    assert!(gone.is_empty());
}

#[test]
fn a_missing_program_is_not_found_and_lookups_answer() {
    let err = spawn(Command {
        arg0: None,
        program: Path::new("/nonexistent/program"),
        args: &[],
        env: &[],
        cwd: Path::new("/"),
        stdin: Stdio::Null,
        stdout: Stdio::Null,
        stderr: Stdio::Null,
        session: false,
    })
    .err()
    .unwrap();
    assert_eq!(err.kind(), io::ErrorKind::NotFound);
    assert_eq!(
        find_on_path("sh", "/nonexistent:/bin"),
        Some(PathBuf::from("/bin/sh"))
    );
    assert_eq!(find_on_path("sh/x", "/bin"), None);
    // A relative entry names the app's directory, not the child's: skipped,
    // even one that leads from here to `/bin`.
    let depth = std::env::current_dir().unwrap().components().count();
    let up = format!("{}bin", "../".repeat(depth));
    assert!(executable(&Path::new(&up).join("sh")));
    assert_eq!(find_on_path("sh", &up), None);
    assert!(executable(Path::new("/bin/sh")));
    assert!(!executable(Path::new("/bin")));
    assert!(home_dir().is_some_and(|h| h.is_absolute()));
    assert!(boot_id().is_some());
    assert_eq!(signal_number("SIGKILL"), Some(libc::SIGKILL));
    assert_eq!(signal_name(libc::SIGTERM), "SIGTERM");
}

#[test]
fn a_session_child_leads_its_own_session() {
    let args = vec![
        "-c".to_string(),
        "ps -o sess= -p $$ >/dev/null; echo $$".to_string(),
    ];
    let child = spawn(Command {
        arg0: None,
        program: Path::new("/bin/sh"),
        args: &args,
        env: &[],
        cwd: Path::new("/"),
        stdin: Stdio::Null,
        stdout: Stdio::Pipe,
        stderr: Stdio::Null,
        session: true,
    })
    .unwrap();
    // SAFETY: plain syscall.
    let sid = unsafe { libc::getsid(child.pid) };
    let out = read_all(child.stdout.unwrap());
    assert_eq!(out.trim(), child.pid.to_string());
    assert!(sid == child.pid || sid == -1, "{sid}");
    let _ = wait(child.pid);
}

#[test]
fn a_write_to_a_child_that_has_gone_fails_without_a_signal() {
    let mut child = sh("echo $EXACT_TEST_TAG; exit 0", Stdio::Pipe);
    let stdin = child.stdin.take().unwrap();
    #[cfg(target_vendor = "apple")]
    assert!(
        sigpipe_off(stdin.as_raw_fd()),
        "the parent's end raises no SIGPIPE"
    );
    let _ = read_all(child.stdout.take().unwrap());
    wait(child.pid).unwrap();
    let mut file = std::fs::File::from(stdin);
    let e = file.write_all(&[0u8; 65536]).unwrap_err();
    assert_eq!(e.raw_os_error(), Some(libc::EPIPE));
    // A child is in this process's session, and names the program as told.
    let args = vec!["5".to_string()];
    let sleeper = spawn(Command {
        arg0: Some("named"),
        program: Path::new("/bin/sleep"),
        args: &args,
        env: &[],
        cwd: Path::new("/"),
        stdin: Stdio::Null,
        stdout: Stdio::Null,
        stderr: Stdio::Null,
        session: false,
    })
    .unwrap();
    assert_eq!(
        session_of(sleeper.pid),
        session_of(std::process::id() as i32)
    );
    assert!(session_of(sleeper.pid).is_some());
    signal_group(sleeper.pid, SIGKILL).unwrap();
    wait(sleeper.pid).unwrap();
}
