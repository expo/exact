//! Tests Observe's Linux launch part (`modules/observe/linux/launch.rs`)
//! against a local sink. A separate test binary because it installs a panic
//! hook and sets `XDG_STATE_HOME`.

mod observe {
    include!("../../../../modules/observe/linux/launch.rs");

    #[cfg(test)]
    mod tests;
    #[cfg(test)]
    mod wire;
}
