//! `posix_spawn` with Apple's attributes (exact2 LLP 1133.007 D9): its own
//! process group (`POSIX_SPAWN_SETPGROUP`) or session (`POSIX_SPAWN_SETSID`),
//! only the descriptors the file actions place (`POSIX_SPAWN_CLOEXEC_DEFAULT`),
//! every signal's default disposition (`POSIX_SPAWN_SETSIGDEF`) and an empty
//! mask (`POSIX_SPAWN_SETSIGMASK`).
use super::Given;
use std::ffi::{CStr, CString};
use std::io;
use std::os::fd::AsRawFd;

/// `<spawn.h>`: Apple's own flags, which libc 0.2.189 lacks in part.
const POSIX_SPAWN_SETSID: libc::c_short = 0x0400;
const POSIX_SPAWN_CLOEXEC_DEFAULT: libc::c_short = 0x4000;

extern "C" {
    /// macOS 10.15 and later.
    fn posix_spawn_file_actions_addchdir_np(
        actions: *mut libc::posix_spawn_file_actions_t,
        path: *const libc::c_char,
    ) -> libc::c_int;
}

fn check(r: libc::c_int) -> io::Result<()> {
    if r == 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(r))
    }
}

fn pointers(strings: &[CString]) -> Vec<*mut libc::c_char> {
    (strings.iter())
        .map(|s| s.as_ptr() as *mut libc::c_char)
        .chain(std::iter::once(std::ptr::null_mut()))
        .collect()
}

pub(super) fn spawn(
    program: &CStr,
    argv: &[CString],
    envp: &[CString],
    cwd: &CStr,
    given: &[Given],
    session: bool,
) -> io::Result<i32> {
    let null = c"/dev/null";
    // SAFETY: each posix_spawn object is initialised before use and
    // destroyed on every path; the strings outlive the call.
    unsafe {
        let mut attr: libc::posix_spawnattr_t = std::mem::zeroed();
        check(libc::posix_spawnattr_init(&mut attr))?;
        let mut actions: libc::posix_spawn_file_actions_t = std::mem::zeroed();
        if let Err(e) = check(libc::posix_spawn_file_actions_init(&mut actions)) {
            libc::posix_spawnattr_destroy(&mut attr);
            return Err(e);
        }
        let result = (|| {
            let mut flags = (libc::POSIX_SPAWN_SETSIGDEF | libc::POSIX_SPAWN_SETSIGMASK)
                as libc::c_short
                | POSIX_SPAWN_CLOEXEC_DEFAULT;
            if session {
                flags |= POSIX_SPAWN_SETSID;
            } else {
                flags |= libc::POSIX_SPAWN_SETPGROUP as libc::c_short;
                check(libc::posix_spawnattr_setpgroup(&mut attr, 0))?;
            }
            check(libc::posix_spawnattr_setflags(&mut attr, flags))?;
            let mut all: libc::sigset_t = std::mem::zeroed();
            libc::sigfillset(&mut all);
            check(libc::posix_spawnattr_setsigdefault(&mut attr, &all))?;
            let mut none: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut none);
            check(libc::posix_spawnattr_setsigmask(&mut attr, &none))?;
            for (fd, given) in given.iter().enumerate() {
                match given {
                    Given::Fd(owned) => check(libc::posix_spawn_file_actions_adddup2(
                        &mut actions,
                        owned.as_raw_fd(),
                        fd as libc::c_int,
                    ))?,
                    Given::Null => check(libc::posix_spawn_file_actions_addopen(
                        &mut actions,
                        fd as libc::c_int,
                        null.as_ptr(),
                        libc::O_RDWR,
                        0,
                    ))?,
                }
            }
            check(posix_spawn_file_actions_addchdir_np(
                &mut actions,
                cwd.as_ptr(),
            ))?;
            let argv = pointers(argv);
            let envp = pointers(envp);
            let mut pid: libc::pid_t = 0;
            check(libc::posix_spawn(
                &mut pid,
                program.as_ptr(),
                &actions,
                &attr,
                argv.as_ptr(),
                envp.as_ptr(),
            ))?;
            Ok(pid)
        })();
        libc::posix_spawn_file_actions_destroy(&mut actions);
        libc::posix_spawnattr_destroy(&mut attr);
        result
    }
}

pub(super) fn start_time(pid: i32) -> Option<u64> {
    // SAFETY: a zeroed proc_bsdinfo is valid, and proc_pidinfo writes at
    // most its size.
    unsafe {
        let mut info: libc::proc_bsdinfo = std::mem::zeroed();
        let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
        let n = libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdinfo).cast(),
            size,
        );
        (n == size).then(|| info.pbi_start_tvsec * 1_000_000 + info.pbi_start_tvusec)
    }
}

/// This boot's identity: `kern.bootsessionuuid`, which `kern.boottime`
/// is not, since a clock correction moves the boot time it reports.
pub(super) fn boot_id() -> Option<String> {
    let mut buf = [0u8; 64];
    let mut size = buf.len();
    let name = c"kern.bootsessionuuid";
    // SAFETY: the out buffer holds `size` bytes.
    let r = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            buf.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if r != 0 {
        return None;
    }
    let text = &buf[..size.min(buf.len())];
    let end = text.iter().position(|b| *b == 0).unwrap_or(text.len());
    std::str::from_utf8(&text[..end])
        .ok()
        .map(str::to_string)
        .filter(|s| !s.is_empty())
}

pub(super) fn group_members(pgid: i32) -> Vec<i32> {
    group_members_with(|pids| {
        let bytes = std::mem::size_of_val(pids) as libc::c_int;
        let buffer = if pids.is_empty() {
            std::ptr::null_mut()
        } else {
            pids.as_mut_ptr().cast()
        };
        // SAFETY: the buffer holds `bytes` bytes, or is null for the count query.
        unsafe { libc::proc_listpgrppids(pgid, buffer, bytes) }
    })
}

fn group_members_with(mut list: impl FnMut(&mut [libc::pid_t]) -> libc::c_int) -> Vec<i32> {
    // libproc takes bytes but returns PID counts, including its null-buffer estimate.
    let mut count = list(&mut []);
    let max_pids = libc::c_int::MAX as usize / std::mem::size_of::<libc::pid_t>();
    let mut pids = Vec::new();
    loop {
        if count <= 0 || count as usize > max_pids {
            return Vec::new();
        }
        pids.resize(count as usize, 0);
        let n = list(&mut pids);
        if n <= 0 {
            return Vec::new();
        }
        if (n as usize) < pids.len() {
            pids.truncate(n as usize);
            pids.retain(|p| *p > 0);
            return pids;
        }
        // A full buffer may omit members that joined after the estimate.
        count = count.saturating_mul(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_members_exceeds_the_old_fixed_buffer() {
        let expected: Vec<_> = (1..=5000).collect();
        let actual = group_members_with(|pids| {
            if pids.is_empty() {
                return expected.len() as libc::c_int;
            }
            let n = pids.len().min(expected.len());
            pids[..n].copy_from_slice(&expected[..n]);
            n as libc::c_int
        });
        assert_eq!(actual.len(), expected.len());
        assert_eq!(actual, expected);
    }

    #[test]
    fn group_members_retries_growth_and_an_exact_fit() {
        let expected = [11, 12, 13, 14];
        let mut sizes = Vec::new();
        let actual = group_members_with(|pids| {
            sizes.push(pids.len());
            if pids.is_empty() {
                return 2;
            }
            let n = pids.len().min(expected.len());
            pids[..n].copy_from_slice(&expected[..n]);
            n as libc::c_int
        });
        assert_eq!(actual, expected);
        assert_eq!(sizes, [0, 2, 4, 8]);
    }

    #[test]
    fn group_members_uses_the_returned_count_instead_of_a_terminator() {
        let actual = group_members_with(|pids| {
            if pids.is_empty() {
                return 5;
            }
            pids.copy_from_slice(&[11, 0, 13, 99, 100]);
            3
        });
        assert_eq!(actual, [11, 13]);
    }

    #[test]
    fn group_members_returns_no_partial_list_on_disappearance_or_failure() {
        for replies in [
            vec![(0, 0)],
            vec![(0, -1)],
            vec![(0, libc::c_int::MAX)],
            vec![(0, 4), (4, 0)],
            vec![(0, 4), (4, -1)],
            vec![(0, 2), (2, 2), (4, 0)],
            vec![(0, 2), (2, 2), (4, -1)],
        ] {
            let mut replies = replies.into_iter();
            let actual = group_members_with(|pids| {
                let (size, result) = replies.next().expect("an expected libproc call");
                assert_eq!(pids.len(), size);
                pids.fill(17);
                result
            });
            assert!(actual.is_empty());
            assert_eq!(replies.next(), None);
        }
    }
}
