# Apple getifaddrs netmask parser reads past sa_len in unsafe address() on compressed BSD sockaddrs

**Status:** Closed
**Resolution:** Enforced sa_len bounds in reach::interfaces::address and added reach::interfaces::netmask to zero-fill omitted trailing bytes and accept AF_UNSPEC without reading past sa_len.
**Systems:** exact-executor (Apple local-network subnet gate in reach.rs)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Trust boundaries)

Verification: verified by running.

At `9c0431fd5`, `unsafe fn interfaces::address(sa: *const SockAddr)` in `host/executor/src/reach.rs:140-152` matches on `sa.family` and unconditionally reads 4 bytes at `bytes.add(4)` for `AF_INET` (offsets `4..8`, requiring `sa_len >= 8`) or 16 bytes at `bytes.add(8)` for `AF_INET6` (offsets `8..24`, requiring `sa_len >= 24`) without ever inspecting `sa.len` (`SockAddr.len` at `reach.rs:119` is unread). `interfaces::on_broadcast_subnet` (`reach.rs:164-188`) calls `address(entry.addr)` and `address(entry.netmask)` (`reach.rs:177`) for every active broadcast-capable interface (`IFF_UP | IFF_BROADCAST`, excluding loopback and point-to-point).

**Triggering sequence:** On macOS 15+ or iOS 14+ where `local_network::prompts() == true` and `reach.substitute.is_none()`, any `Tool`, `Wake`, or `Device` fetch or WebSocket (wired via `reach::gate_fetch` at `lib.rs:1299`, `async_http.rs:46`, `preload.rs:1378`, `preload_fetch.rs:297`; `reach::gate_bindings` at `stream.rs:122`; `reach::admit_cached` at `cache.rs:505`, `lib.rs:1376`) to any non-loopback, non-link-local, non-multicast, non-broadcast unicast IP (or domain resolving to one) runs `Gate::admit` (`reach.rs:25`) -> `at_risk` (`:66`) -> `address_risk` (`:84`) -> `interfaces::on_broadcast_subnet` (`:164`) -> `address(entry.netmask)` (`:177`).

**Actual vs. expected:** On Apple (BSD/XNU), `getifaddrs(3)` trims trailing zero bytes from `ifa_netmask` `sockaddr`s and packs `sockaddr`s back-to-back at 4-byte (`SA_RLEN`) alignment with `ifa_addr` immediately following `ifa_netmask`. For every `/24`, `/16`, or `/8` IPv4 interface (`sa_len` = 7, 6, or 5), `read_unaligned(bytes.add(4) as *const [u8; 4])` reads 1–3 bytes past `sa_len` (violating `address()`'s own `# Safety` precondition at `reach.rs:139`: "`sa` is null or points at a `sockaddr` of at least its `sa_len` bytes"). While `/24`/`/16`/`/8` fall within the zeroed 8-byte aligned `SA_RLEN` slot on current macOS, any compressed `AF_INET` netmask with `sa_len <= 4` (`ROUNDUP(sa_len, 4) == 4`) or compressed `AF_INET6` netmask with `sa_len < 24` (e.g., `sa_len == 16` for `/64`) reads directly across the boundary into the adjacent `sockaddr` (`ifa_addr`) or past the allocation, corrupting the subnet mask enforced by the TN3179 local-network prompt gate. Additionally, a BSD `ifa_netmask` carrying `sa_family == 0` (`AF_UNSPEC`) causes `address()` to return `None` and skip the interface's subnet. Expected: `interfaces::address` and netmask parsing must honor `sa.len`, never reading past `sa_len` bytes, zero-filling omitted trailing netmask bytes, and falling back to `entry.addr`'s family when `netmask.sa_family == 0`.

**Verification and sibling sweep:** Verified by running a Python `ctypes` `getifaddrs(3)` probe on this macOS host (including with `MallocScribble=1 MallocPreScribble=1`):
- `en0 flags=0x8863 af=2 alen=16 aptr=0x7c0ed82060 | mask: mf=2 mlen=7 mptr=0x7c0ed82058 diff(a-m)=8 raw=07020000ffffff00` (broadcast-capable interface matched by `reach.rs:175` with `sa_len = 7 < 8`, `ifa_addr` immediately at `+8`).
- `lo0 flags=0x8049 af=2 alen=16 aptr=0x7c0ed81040 | mask: mf=2 mlen=5 mptr=0x7c0ed81038 diff(a-m)=8 raw=05020000ff000000`.
Checked all `getifaddrs` / `sockaddr` sites across `host/`, `crates/`, and `kernel/`: `host/executor/src/reach.rs` is the sole `getifaddrs(3)` caller and the sole unsafe `sockaddr` reader in the repository (Swift `sockaddr_in`/`sockaddr_un` code in `host/apple/` only constructs fixed-size local structs for `bind`/`connect`/`getsockname`).

**Suggested fix and regression test:** Separate `address(sa)` (checking `sa.len >= 8` for `AF_INET` and `sa.len >= 24` for `AF_INET6` before reading) from `netmask(sa, expected_family)` (accepting `sa.family == expected_family || sa.family == 0`, copying `usize::from(sa.len).saturating_sub(offset).min(len)` bytes via `std::slice::from_raw_parts` into a zero-initialized `[u8; 4]` or `[u8; 16]`). Add unit tests in `host/executor/src/reach.rs` feeding synthetic packed byte arrays with compressed IPv4 (`sa_len = 0, 4, 5, 6, 7, 8`, `sa_family = 0` and `AF_INET`) and IPv6 (`sa_len = 0, 8, 16, 24, 28`) netmasks followed immediately by `0xFF` sentinel bytes to verify omitted trailing bytes decode as zero and bytes past `sa_len` are never read.

### Siblings checked

Searched all getifaddrs, ifa_netmask, sa_len, and sockaddr sites across host/, crates/, and kernel/; host/executor/src/reach.rs is the only getifaddrs(3) caller and unsafe sockaddr parser.
