//! The plan, compatibility document and module bytecode the link carries
//! beside the archive.
//!
//! A TypeScript app's Apple build bakes outside Cargo and links the plan, its
//! `compat.json` and the module's Hermes bytecode into the executable's
//! `__TEXT` segment (`ld -sectcreate`, `host/apple/build.mjs`), so an edit to
//! the Contract or the TypeScript alone links again and compiles no Rust. All
//! three stay in the signed executable, as they were in the archive; reading
//! one is a pointer into the mapped image (`getsectiondata(3)`), well under a
//! microsecond.

/// The linked plan. Panics when the link carried none: an archive whose
/// entry reads its plan here is linked by `build.mjs`, which always adds it.
pub fn plan() -> &'static [u8] {
    section(c"__exact_plan")
        .expect("no __TEXT,__exact_plan: host/apple/build.mjs links this archive's plan")
}

/// The linked compatibility document (`compat.json`); panics as [`plan`] does.
pub fn compat() -> &'static str {
    static TEXT: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();
    TEXT.get_or_init(|| {
        let bytes = section(c"__exact_compat").expect(
            "no __TEXT,__exact_compat: host/apple/build.mjs links this archive's compat.json",
        );
        std::str::from_utf8(bytes).expect("the linked compat.json is UTF-8")
    })
}

/// The module's identity the bake wrote into the linked compat.json
/// (`"module"`, js/bake `with_module_identity`): its app id, grants and
/// Canvas 2D roster. They travel with the plan in the signed executable, so
/// an archive's Rust need not name them, and apps that differ only there
/// share one compiled archive. Read once; the bake writes `"module"` last,
/// so only that tail is parsed (the whole document if it is elsewhere).
pub struct Identity {
    /// The module's app id (`APP`).
    pub app: &'static str,
    /// The module's grants, one per line (`GRANTS`).
    pub grants: &'static str,
    /// The Canvas 2D roster: name and arity (`CANVAS_SURFACES`).
    pub canvas_surfaces: &'static [(&'static str, usize)],
}

/// The linked [`Identity`]; panics as [`plan`] does when the link carried a
/// compat.json without one (a bake from before it was written there).
pub fn identity() -> &'static Identity {
    static IDENTITY: std::sync::OnceLock<Identity> = std::sync::OnceLock::new();
    IDENTITY.get_or_init(|| identity_from(compat()).expect("the linked compat.json names no module identity: bake again"))
}

fn identity_from(compat: &str) -> Option<Identity> {
    let compat = compat.trim_end();
    let tail = compat
        .rfind(",\"module\":")
        .and_then(|at| compat.get(at + 10..compat.len().checked_sub(1)?))
        .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok());
    let module = match tail {
        Some(module) => module,
        None => serde_json::from_str::<serde_json::Value>(compat).ok()?.get("module")?.clone(),
    };
    let leak = |text: &str| -> &'static str { Box::leak(text.to_owned().into_boxed_str()) };
    let surfaces: Vec<(&'static str, usize)> = module["canvasSurfaces"]
        .as_array()?
        .iter()
        .map(|pair| Some((leak(pair[0].as_str()?), pair[1].as_u64()? as usize)))
        .collect::<Option<_>>()?;
    Some(Identity {
        app: leak(module["app"].as_str()?),
        grants: leak(module["grants"].as_str()?),
        canvas_surfaces: Box::leak(surfaces.into_boxed_slice()),
    })
}

/// `APP` and `GRANTS` of a linked bake's `module.rs`: read from
/// [`identity`] where they are used, as the `&str` constants were.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Linked {
    /// The module's app id.
    App,
    /// The module's grants.
    Grants,
}

impl Linked {
    /// The text this names.
    pub fn as_str(self) -> &'static str {
        match self {
            Linked::App => identity().app,
            Linked::Grants => identity().grants,
        }
    }
}

impl std::ops::Deref for Linked {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<str> for Linked {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for Linked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<Linked> for String {
    fn from(linked: Linked) -> String {
        linked.as_str().to_owned()
    }
}

impl PartialEq<&str> for Linked {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

/// `CANVAS_SURFACES` of a linked bake's `module.rs`: [`identity`]'s roster.
#[derive(Clone, Copy, Debug)]
pub struct LinkedSurfaces;

impl std::ops::Deref for LinkedSurfaces {
    type Target = [(&'static str, usize)];
    fn deref(&self) -> &Self::Target {
        identity().canvas_surfaces
    }
}

impl AsRef<[(&'static str, usize)]> for LinkedSurfaces {
    fn as_ref(&self) -> &[(&'static str, usize)] {
        identity().canvas_surfaces
    }
}

/// The linked module bytecode, 16-byte aligned (`-sectalign`), as Hermes
/// reads it in place; panics as [`plan`] does.
pub fn bytecode() -> &'static [u8] {
    section(c"__exact_hbc")
        .expect("no __TEXT,__exact_hbc: host/apple/build.mjs links this archive's module")
}

/// The bytes of `__TEXT,<name>` in the image this code is linked into (the
/// app's executable, or the framework that embeds it).
#[cfg(target_vendor = "apple")]
#[allow(unsafe_code)]
fn section(name: &std::ffi::CStr) -> Option<&'static [u8]> {
    use std::ffi::{c_char, c_void};
    extern "C" {
        /// The linker's symbol for the Mach-O header of the image defining it.
        static __dso_handle: u8;
        fn getsectiondata(
            header: *const c_void,
            segment: *const c_char,
            section: *const c_char,
            size: *mut usize,
        ) -> *mut u8;
    }
    let mut size = 0usize;
    // SAFETY: `__dso_handle` is this image's mapped header, which lives as long
    // as the process; `getsectiondata` returns a pointer into that mapping and
    // its length, or null.
    unsafe {
        let header = std::ptr::addr_of!(__dso_handle).cast::<c_void>();
        let data = getsectiondata(header, c"__TEXT".as_ptr(), name.as_ptr(), &mut size);
        (!data.is_null()).then(|| std::slice::from_raw_parts(data, size))
    }
}

#[cfg(not(target_vendor = "apple"))]
fn section(_: &std::ffi::CStr) -> Option<&'static [u8]> {
    None
}

#[cfg(test)]
mod tests {
    use super::identity_from;

    #[test]
    fn reads_the_module_identity_from_the_tail_or_the_whole_document() {
        let compat = r#"{"id":"ab","inputs":{"app":"x","module":"not this"},"module":{"app":"com.example.a","canvasSurfaces":[["jar",6]],"grants":"fs.read app:/data/a.json\nnet.fetch https://e.example"}}
"#;
        let identity = identity_from(compat).expect("an identity");
        assert_eq!(identity.app, "com.example.a");
        assert_eq!(identity.grants, "fs.read app:/data/a.json\nnet.fetch https://e.example");
        assert_eq!(identity.canvas_surfaces, &[("jar", 6)]);
        let elsewhere = r#"{"module":{"app":"b","canvasSurfaces":[],"grants":""},"id":"cd"}"#;
        assert_eq!(identity_from(elsewhere).expect("an identity").app, "b");
        assert!(identity_from(r#"{"id":"ab"}"#).is_none());
    }
}
