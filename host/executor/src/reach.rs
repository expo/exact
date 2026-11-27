//! LLP 1133.036 D6 at every fetch hop and WebSocket open this executor makes:
//! a reach asked by a tool invocation, a wake or a package's event, whose
//! destination could show the Local Network prompt, is checked against the
//! host's local-network fact before it connects ([`exact_runner::local_network`]).
//!
//! The test of the destination is the host's. Under the agent it is the
//! grant's static test, since the substitute reads no interface (D19). In
//! production on Apple it is TN3179's: a `.local` name; a link-local,
//! multicast or broadcast address; an address on the subnet of an interface
//! that can broadcast (Wi-Fi, Ethernet; not a VPN's point-to-point tunnel or
//! cellular); a unicast name that resolves to one of those. A name is
//! resolved here for the test and again by the transport when it connects,
//! and an answer that changes between the two escapes the test, a declared
//! limit (D6).

use exact_runner::local_network::{self, Reach};
use exact_runner::CallOrigin;
use ibex2::boundary::{HopGate, HostError};
use std::net::{IpAddr, ToSocketAddrs};
use std::sync::Arc;

struct Gate(Reach);

impl HopGate for Gate {
    fn admit(&self, url: &url::Url) -> Result<(), HostError> {
        let risk = local_network::prompts() && url.host().is_some_and(|h| at_risk(&h, self.0));
        match self.0.refusal(risk) {
            None => Ok(()),
            Some(reason) => Err(HostError::NotAllowed {
                reason: reason.into(),
            }),
        }
    }
}

pub(super) fn admit_cached(request: &exact_runner::Request) -> Result<(), HostError> {
    if request.reach.origin == CallOrigin::App {
        return Ok(());
    }
    let url = url::Url::parse(&request.url).map_err(|e| HostError::Failed(e.to_string()))?;
    Gate(request.reach).admit(&url)
}

/// `fetch`, checked at every hop for `reach`: unchanged for a person's or
/// the app's own reach, which the rule never refuses.
pub(super) fn gate_fetch(fetch: ibex2::host::Fetch, reach: Reach) -> ibex2::host::Fetch {
    match reach.origin {
        CallOrigin::App => fetch,
        _ => fetch.with_gate(Arc::new(Gate(reach))),
    }
}

/// `bindings`, their fetch and WebSocket checked for `reach`.
pub(super) fn gate_bindings(
    mut bindings: ibex2::host::Bindings,
    reach: Reach,
) -> ibex2::host::Bindings {
    if reach.origin != CallOrigin::App {
        let gate: Arc<dyn HopGate> = Arc::new(Gate(reach));
        bindings.fetch = bindings.fetch.with_gate(gate.clone());
        bindings.websocket = bindings.websocket.with_gate(gate);
    }
    bindings
}

fn at_risk(host: &url::Host<&str>, reach: Reach) -> bool {
    if reach.substitute.is_some() {
        return Reach::static_risk(&host.to_string());
    }
    match host {
        url::Host::Ipv4(a) => address_risk(IpAddr::V4(*a)),
        url::Host::Ipv6(a) => address_risk(IpAddr::V6(*a)),
        url::Host::Domain(name) => {
            let name = name.strip_suffix('.').unwrap_or(name);
            name.ends_with(".local")
                || (name, 0)
                    .to_socket_addrs()
                    .is_ok_and(|mut found| found.any(|a| address_risk(a.ip())))
        }
    }
}

/// Whether reaching `address` can need local network access (TN3179).
fn address_risk(address: IpAddr) -> bool {
    let address = match address {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(address, IpAddr::V4),
        v4 => v4,
    };
    match address {
        _ if address.is_loopback() || address.is_unspecified() => false,
        IpAddr::V4(a) if a.is_link_local() || a.is_multicast() || a.is_broadcast() => true,
        IpAddr::V6(a) if a.is_multicast() || (a.segments()[0] & 0xffc0) == 0xfe80 => true,
        _ => interfaces::on_broadcast_subnet(address),
    }
}

#[cfg(target_vendor = "apple")]
#[allow(unsafe_code)] // getifaddrs(3), whose list is read and freed here.
mod interfaces {
    //! The device's broadcast-capable interfaces, by `getifaddrs(3)`.
    use super::IpAddr;
    use std::net::{Ipv4Addr, Ipv6Addr};
    use std::os::raw::{c_char, c_int, c_uint, c_void};

    #[repr(C)]
    struct IfAddrs {
        next: *mut IfAddrs,
        name: *mut c_char,
        flags: c_uint,
        addr: *mut SockAddr,
        netmask: *mut SockAddr,
        dstaddr: *mut SockAddr,
        data: *mut c_void,
    }

    /// `sockaddr_in` and `sockaddr_in6` share this head on Apple platforms.
    #[repr(C)]
    #[allow(dead_code)] // Fields are read through byte offsets gated by `sa_len`.
    pub(super) struct SockAddr {
        len: u8,
        family: u8,
    }

    extern "C" {
        fn getifaddrs(out: *mut *mut IfAddrs) -> c_int;
        fn freeifaddrs(list: *mut IfAddrs);
    }

    const AF_INET: u8 = 2;
    const AF_INET6: u8 = 30;
    const IFF_UP: c_uint = 0x1;
    const IFF_BROADCAST: c_uint = 0x2;
    const IFF_LOOPBACK: c_uint = 0x8;
    const IFF_POINTOPOINT: c_uint = 0x10;

    /// The address `sa` holds, by its family: `sin_addr` at byte 4 (requires
    /// `sa_len >= 8`), `sin6_addr` at byte 8 (requires `sa_len >= 24`).
    ///
    /// # Safety
    /// `sa` is null or points at a `sockaddr` with at least `max(1, sa_len)`
    /// readable bytes.
    pub(super) unsafe fn address(sa: *const SockAddr) -> Option<IpAddr> {
        if sa.is_null() {
            return None;
        }
        let bytes = sa as *const u8;
        let sa_len = usize::from(*bytes);
        if sa_len < 2 {
            return None;
        }
        match *bytes.add(1) {
            AF_INET if sa_len >= 8 => {
                let mut octets = [0u8; 4];
                octets.copy_from_slice(std::slice::from_raw_parts(bytes.add(4), 4));
                Some(IpAddr::V4(Ipv4Addr::from(octets)))
            }
            AF_INET6 if sa_len >= 24 => {
                let mut octets = [0u8; 16];
                octets.copy_from_slice(std::slice::from_raw_parts(bytes.add(8), 16));
                Some(IpAddr::V6(Ipv6Addr::from(octets)))
            }
            _ => None,
        }
    }

    /// A BSD/XNU `ifa_netmask` `sockaddr`, whose trailing zero bytes may be
    /// omitted by `sa_len` and whose `sa_family` may be `0` (`AF_UNSPEC`).
    /// Omitted trailing bytes decode as zero and bytes past `sa_len` are never read.
    ///
    /// # Safety
    /// `sa` is null or points at a `sockaddr` with at least `max(1, sa_len)`
    /// readable bytes.
    pub(super) unsafe fn netmask(sa: *const SockAddr, expected_family: u8) -> Option<IpAddr> {
        if sa.is_null() {
            return None;
        }
        let bytes = sa as *const u8;
        let sa_len = usize::from(*bytes);
        if sa_len >= 2 {
            let family = *bytes.add(1);
            if family != expected_family && family != 0 {
                return None;
            }
        }
        match expected_family {
            AF_INET => {
                let mut octets = [0u8; 4];
                let count = sa_len.saturating_sub(4).min(4);
                if count > 0 {
                    octets[..count]
                        .copy_from_slice(std::slice::from_raw_parts(bytes.add(4), count));
                }
                Some(IpAddr::V4(Ipv4Addr::from(octets)))
            }
            AF_INET6 => {
                let mut octets = [0u8; 16];
                let count = sa_len.saturating_sub(8).min(16);
                if count > 0 {
                    octets[..count]
                        .copy_from_slice(std::slice::from_raw_parts(bytes.add(8), count));
                }
                Some(IpAddr::V6(Ipv6Addr::from(octets)))
            }
            _ => None,
        }
    }

    fn masked(a: IpAddr, mask: IpAddr) -> Option<u128> {
        match (a, mask) {
            (IpAddr::V4(a), IpAddr::V4(m)) => Some((u32::from(a) & u32::from(m)) as u128),
            (IpAddr::V6(a), IpAddr::V6(m)) => Some(u128::from(a) & u128::from(m)),
            _ => None,
        }
    }

    /// Whether `target` is on the subnet of an interface that is up and can
    /// broadcast, the subnet's broadcast address included.
    pub(super) fn on_broadcast_subnet(target: IpAddr) -> bool {
        let mut list = std::ptr::null_mut();
        // SAFETY: getifaddrs fills `list` on success, and it is freed once.
        if unsafe { getifaddrs(&mut list) } != 0 {
            return false;
        }
        let mut found = false;
        let mut at = list;
        while let Some(entry) = unsafe { at.as_ref() } {
            let wanted = IFF_UP | IFF_BROADCAST;
            let flags = entry.flags;
            if flags & wanted == wanted && flags & (IFF_LOOPBACK | IFF_POINTOPOINT) == 0 {
                // SAFETY: the kernel's entries hold sockaddrs or null.
                if let Some(addr) = unsafe { address(entry.addr) } {
                    let family = match addr {
                        IpAddr::V4(_) => AF_INET,
                        IpAddr::V6(_) => AF_INET6,
                    };
                    // SAFETY: `entry.netmask` is null or a kernel sockaddr.
                    if let Some(mask) = unsafe { netmask(entry.netmask, family) } {
                        let (subnet, theirs) = (masked(addr, mask), masked(target, mask));
                        found |= subnet.is_some() && subnet == theirs;
                    }
                }
            }
            at = entry.next;
        }
        // SAFETY: `list` came from getifaddrs.
        unsafe { freeifaddrs(list) };
        found
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn packed(prefix: &[u8]) -> Vec<u8> {
            let mut buf = prefix.to_vec();
            // Trailing 0xFF sentinels immediately after `sa_len` detect any over-read.
            buf.extend_from_slice(&[0xff; 32]);
            buf
        }

        #[test]
        fn compressed_ipv4_and_ipv6_netmasks_never_read_past_sa_len() {
            for family in [0u8, AF_INET] {
                let cases: &[(&[u8], Ipv4Addr)] = &[
                    (&[0], Ipv4Addr::new(0, 0, 0, 0)),
                    (&[1], Ipv4Addr::new(0, 0, 0, 0)),
                    (&[4, family, 0, 0], Ipv4Addr::new(0, 0, 0, 0)),
                    (&[5, family, 0, 0, 0xff], Ipv4Addr::new(255, 0, 0, 0)),
                    (
                        &[6, family, 0, 0, 0xff, 0xff],
                        Ipv4Addr::new(255, 255, 0, 0),
                    ),
                    (
                        &[7, family, 0, 0, 0xff, 0xff, 0xff],
                        Ipv4Addr::new(255, 255, 255, 0),
                    ),
                    (
                        &[8, family, 0, 0, 0xff, 0xff, 0xff, 0xf0],
                        Ipv4Addr::new(255, 255, 255, 240),
                    ),
                ];
                for (raw, expected) in cases {
                    let buf = packed(raw);
                    // SAFETY: `buf` has at least `max(1, sa_len)` bytes.
                    let got = unsafe { netmask(buf.as_ptr().cast(), AF_INET) };
                    assert_eq!(got, Some(IpAddr::V4(*expected)), "raw={raw:?}");
                }
            }

            for family in [0u8, AF_INET6] {
                let mut len16 = vec![16, family, 0, 0, 0, 0, 0, 0];
                len16.extend_from_slice(&[0xff; 8]);
                let mut len24 = vec![24, family, 0, 0, 0, 0, 0, 0];
                len24.extend_from_slice(&[
                    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0, 0, 0, 0, 0, 0,
                ]);
                let mut len28 = len24.clone();
                len28[0] = 28;
                len28.extend_from_slice(&[0, 0, 0, 0]);
                let cases: &[(&[u8], Ipv6Addr)] = &[
                    (&[0], Ipv6Addr::UNSPECIFIED),
                    (&[8, family, 0, 0, 0, 0, 0, 0], Ipv6Addr::UNSPECIFIED),
                    (
                        &len16,
                        Ipv6Addr::new(0xffff, 0xffff, 0xffff, 0xffff, 0, 0, 0, 0),
                    ),
                    (
                        &len24,
                        Ipv6Addr::new(0xffff, 0xffff, 0xffff, 0xffff, 0xffff, 0, 0, 0),
                    ),
                    (
                        &len28,
                        Ipv6Addr::new(0xffff, 0xffff, 0xffff, 0xffff, 0xffff, 0, 0, 0),
                    ),
                ];
                for (raw, expected) in cases {
                    let buf = packed(raw);
                    // SAFETY: `buf` has at least `max(1, sa_len)` bytes.
                    let got = unsafe { netmask(buf.as_ptr().cast(), AF_INET6) };
                    assert_eq!(got, Some(IpAddr::V6(*expected)), "raw={raw:?}");
                }
            }

            // Mismatched non-zero family is rejected.
            let wrong = packed(&[8, AF_INET6, 0, 0, 0xff, 0xff, 0xff, 0]);
            assert_eq!(unsafe { netmask(wrong.as_ptr().cast(), AF_INET) }, None);
            assert_eq!(unsafe { netmask(std::ptr::null(), AF_INET) }, None);
        }

        #[test]
        fn address_rejects_truncated_sockaddrs() {
            for truncated in [
                &[0u8][..],
                &[1],
                &[4, AF_INET, 0, 0],
                &[7, AF_INET, 0, 0, 192, 168, 1],
                &[8, AF_INET6, 0, 0, 0, 0, 0, 0],
                &[
                    23, AF_INET6, 0, 0, 0, 0, 0, 0, 0xfe, 0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0,
                ],
            ] {
                let buf = packed(truncated);
                // SAFETY: `buf` has at least `max(1, sa_len)` bytes.
                assert_eq!(
                    unsafe { address(buf.as_ptr().cast()) },
                    None,
                    "truncated={truncated:?}"
                );
            }
            let valid_v4 = packed(&[8, AF_INET, 0, 0, 192, 168, 1, 40]);
            assert_eq!(
                unsafe { address(valid_v4.as_ptr().cast()) },
                Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 40)))
            );
            let mut valid_v6 = vec![24, AF_INET6, 0, 0, 0, 0, 0, 0];
            valid_v6.extend_from_slice(&Ipv6Addr::new(0xfd00, 0, 0, 0, 0, 0, 0, 1).octets());
            let valid_v6 = packed(&valid_v6);
            assert_eq!(
                unsafe { address(valid_v6.as_ptr().cast()) },
                Some(IpAddr::V6(Ipv6Addr::new(0xfd00, 0, 0, 0, 0, 0, 0, 1)))
            );
        }
    }
}

#[cfg(not(target_vendor = "apple"))]
mod interfaces {
    /// No target but Apple's prompts, so no other host asks.
    pub(super) fn on_broadcast_subnet(_: super::IpAddr) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_local_multicast_and_loopback_addresses_are_judged_without_interfaces() {
        assert!(address_risk("169.254.3.4".parse().unwrap()));
        assert!(address_risk("224.0.0.251".parse().unwrap()));
        assert!(address_risk("255.255.255.255".parse().unwrap()));
        assert!(address_risk("fe80::1".parse().unwrap()));
        assert!(address_risk("::ffff:169.254.3.4".parse().unwrap()));
        assert!(!address_risk("127.0.0.1".parse().unwrap()));
        assert!(!address_risk("::1".parse().unwrap()));
    }

    #[test]
    fn under_the_agent_the_static_test_decides() {
        let reach = Reach {
            origin: CallOrigin::Tool,
            substitute: Some(local_network::State::Prompt),
        };
        let host = |s: &str| url::Url::parse(s).unwrap();
        let risk = |s: &str| at_risk(&host(s).host().unwrap(), reach);
        assert!(risk("http://192.168.1.40/"));
        assert!(risk("http://wled-a1b2c3.local/"));
        assert!(!risk("http://127.0.0.1:4790/"));
        assert!(!risk("https://example.test/"));
    }
}

#[cfg(test)]
#[path = "reach_tests.rs"]
mod executor_tests;
