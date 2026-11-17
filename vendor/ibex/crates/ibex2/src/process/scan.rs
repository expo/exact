//! A process group's live members among the processes `/proc` lists.

use std::io;

/// The members of group `pgid` among `pids` that `live` says have a live
/// thread. Membership is `group_of`'s answer (`getpgid`, which `/proc`'s
/// `hidepid` does not restrict), so a process outside the group is never
/// read. A member `live` may not read (`EACCES`) counts as live: it may be.
pub(super) fn live_members(
    pids: impl IntoIterator<Item = io::Result<i32>>,
    pgid: i32,
    group_of: impl Fn(i32) -> io::Result<i32>,
    live: impl Fn(i32) -> io::Result<bool>,
) -> io::Result<Vec<i32>> {
    let mut members = Vec::new();
    for pid in pids {
        let pid = pid?;
        match group_of(pid) {
            Ok(group) if group == pgid => {}
            Ok(_) => continue,
            Err(e) if vanished(&e) => continue,
            Err(e) => return Err(e),
        }
        match live(pid) {
            Ok(true) => members.push(pid),
            Ok(false) => {}
            Err(e) if vanished(&e) => {}
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => members.push(pid),
            Err(e) => return Err(e),
        }
    }
    Ok(members)
}

/// The process ended between the listing and the read.
pub(super) fn vanished(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::NotFound || error.raw_os_error() == Some(libc::ESRCH)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEADER: i32 = 100;
    const MEMBER: i32 = 101;
    const OTHER: i32 = 200;

    fn error(errno: i32) -> io::Error {
        io::Error::from_raw_os_error(errno)
    }

    fn group_of(pid: i32) -> io::Result<i32> {
        match pid {
            LEADER | MEMBER => Ok(LEADER),
            OTHER => Ok(OTHER),
            _ => Err(error(libc::ESRCH)),
        }
    }

    #[test]
    fn an_unreadable_process_outside_the_group_is_not_inspected() {
        // Under `hidepid=1`, another user's `/proc/<pid>/stat` is EACCES;
        // getpgid still answers for it.
        let live = |pid| match pid {
            LEADER => Ok(false),
            _ => Err(error(libc::EACCES)),
        };
        let members = live_members([Ok(LEADER), Ok(OTHER)], LEADER, group_of, live);
        assert_eq!(members.unwrap(), Vec::<i32>::new());
    }

    #[test]
    fn an_unreadable_member_counts_as_live() {
        let live = |pid| match pid {
            LEADER => Ok(false),
            _ => Err(error(libc::EACCES)),
        };
        let members = live_members([Ok(LEADER), Ok(MEMBER)], LEADER, group_of, live);
        assert_eq!(members.unwrap(), [MEMBER]);
    }

    #[test]
    fn a_process_that_ends_during_the_scan_is_skipped() {
        let gone = |_| Err(error(libc::ESRCH));
        let members = live_members([Ok(LEADER), Ok(300)], LEADER, group_of, gone);
        assert_eq!(members.unwrap(), Vec::<i32>::new());
        let not_found = |_| Err(io::Error::from(io::ErrorKind::NotFound));
        let members = live_members([Ok(MEMBER)], LEADER, group_of, not_found);
        assert_eq!(members.unwrap(), Vec::<i32>::new());
    }

    #[test]
    fn a_failed_listing_or_member_read_is_unknown() {
        let live = |_| Ok(true);
        let listing = live_members(
            [Ok(LEADER), Err(error(libc::EMFILE))],
            LEADER,
            group_of,
            live,
        );
        assert_eq!(listing.unwrap_err().raw_os_error(), Some(libc::EMFILE));
        let read = |_| Err(error(libc::EIO));
        let member = live_members([Ok(MEMBER)], LEADER, group_of, read);
        assert_eq!(member.unwrap_err().raw_os_error(), Some(libc::EIO));
    }

    #[test]
    fn live_members_are_listed_and_ended_ones_are_not() {
        let live = |pid| Ok(pid == MEMBER);
        let members = live_members([Ok(LEADER), Ok(MEMBER), Ok(OTHER)], LEADER, group_of, live);
        assert_eq!(members.unwrap(), [MEMBER]);
    }
}
