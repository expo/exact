# ureq 3.4.0, renamed `exact-ureq`

The crates.io archive's source, licenses, README and `.cargo_vcs_info.json`.
The source is unchanged. `Cargo.toml` names the package `exact-ureq`, lets
the library take that name, and drops the examples and dev-dependencies,
which the archive's manifest names but this copy does not carry.

- Upstream: https://github.com/algesten/ureq
- Release archive: https://static.crates.io/crates/ureq/ureq-3.4.0.crate
- Archive SHA256: 972d7902c8735f2695410b8aed7df6ed12a47394aa1c8d7af49f0497b731a94d
- Upstream VCS revision: 5e803fc4ce031a0d667dc6cdb97d1413ac8cdab9

## Why

`ibex2` (`src/transport/rustls_http.rs`) and `apps/harness/data`
(`src/providers/mod.rs`) implement ureq's `unversioned` resolver and
transport traits, which ureq exempts from semver; 3.5 added fields to
`NextTimeout`. Both take this copy as `ureq = { package = "exact-ureq",
path = … }`, so an app's own `ureq` resolves to whatever version it asks
for (LLP 1153.003 D6.5). The two copies share one rustls 0.23.

Replace the copy, not the code, to move to a newer ureq: vendor the new
release the same way and fix the two implementations against its
`unversioned` module.
