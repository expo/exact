//! Native lengths use the kernel evaluator. Serialized expressions are
//! carried as text in a batch; each Swift style owns its parsed expression and environment.
use exact_kernel::style::Comparison;
use exact_kernel::Env;
/// A calculation and the environment owned by its native style.
pub struct Context {
    calculation: Comparison,
    env: Env,
}
impl Context {
    /// Read canonical CSS and copy its environment. See `exact_calc_create` in exact.h.
    pub fn new(text: &str, values: &[f64]) -> Option<Box<Self>> {
        let calculation = Comparison::from_css(text)?;
        // The layout regions' count, exactly 0 or 20, then the regions (LLP
        // 1123 D10); the title-bar area's, 0 or 4, then the area
        let regions = match values.get(8) {
            Some(&0.0) => 0,
            Some(&20.0) => 20,
            _ => return None,
        };
        let titlebar = match values.get(9 + regions) {
            Some(&0.0) => 0,
            Some(&4.0) => 4,
            _ => return None,
        };
        let segments = 10 + regions + titlebar;
        if values.len() < segments
            || !(values.len() - segments).is_multiple_of(4)
            || !values
                .iter()
                .all(|v| v.is_finite() && (*v as f32).is_finite())
            || values[6..8]
                .iter()
                .any(|v| *v < 1.0 || *v > 255.0 || v.fract() != 0.0)
        {
            return None;
        }
        let mut env = Env::new(
            values[0] as f32,
            values[1] as f32,
            values[2] as f32,
            values[3] as f32,
        );
        env.viewport_width = values[4] as f32;
        env.viewport_height = values[5] as f32;
        env.cols = values[6] as u8;
        env.rows = values[7] as u8;
        let regions = &values[9..9 + regions];
        env.regions = (regions.len() == 20)
            .then(|| std::array::from_fn(|r| std::array::from_fn(|e| regions[r * 4 + e] as f32)));
        env.titlebar_area = match values[10 + regions.len()..segments] {
            [x, y, w, h] => Some(exact_kernel::style::env::Rect::new(
                x as f32, y as f32, w as f32, h as f32,
            )),
            _ => None,
        };
        env.segments = values[segments..]
            .chunks_exact(4)
            .map(|r| {
                exact_kernel::style::env::Rect::new(
                    r[0] as f32,
                    r[1] as f32,
                    r[2] as f32,
                    r[3] as f32,
                )
            })
            .collect();
        env.segments_consistent()
            .then(|| Box::new(Self { calculation, env }))
    }
    /// Resolve without copying the environment or consulting the bridge's handle table.
    pub fn length(&self, basis: f64) -> f64 {
        f64::from(
            self.calculation
                .value(&self.env, Some(basis as f32))
                .unwrap_or(0.0),
        )
    }
}

/// Serialize the fields a calculation reads. Viewport units use the window's
/// dimensions even while the page's layout viewport is a sheet.
pub fn write(out: &mut String, c: Comparison, env: &Env) {
    use std::fmt::Write;
    let (width, height) = env
        .screen
        .unwrap_or((env.viewport_width, env.viewport_height));
    let mut text = String::new();
    c.css(&mut text);
    let text = serde_json::to_string(&text).expect("calculation text");
    let _ = write!(
        out,
        "{{\"calc\":{},\"env\":[{},{},{},{},{},{},{},{}",
        text, env.top, env.right, env.bottom, env.left, width, height, env.cols, env.rows
    );
    let regions = env.regions.as_ref().map_or(&[][..], |r| r.as_flattened());
    let _ = write!(out, ",{}", regions.len());
    for v in regions {
        let _ = write!(out, ",{v}");
    }
    match &env.titlebar_area {
        Some(r) => {
            let _ = write!(out, ",4,{},{},{},{}", r.x, r.y, r.width, r.height);
        }
        None => out.push_str(",0"),
    }
    for r in &env.segments {
        let _ = write!(out, ",{},{},{},{}", r.x, r.y, r.width, r.height);
    }
    out.push_str("]}");
}

/// Export the calculation seam from the application's static archive.
#[macro_export]
macro_rules! calc_exports {
    () => {
        /// Parse a style's expression and copy its environment; null for invalid data.
        /// # Safety
        /// `text` holds `len` bytes; `values` holds `count` aligned doubles.
        #[no_mangle]
        pub unsafe extern "C" fn exact_calc_create(
            text: *const u8,
            len: usize,
            values: *const f64,
            count: usize,
        ) -> *mut ::std::ffi::c_void {
            // Eight values, the regions' count and twenty regions, the title-bar
            // area's count and its four values, then up to 255×255 segments
            if text.is_null() || values.is_null() || count > 9 + 20 + 1 + 4 + 4 * 255 * 255 {
                return ::std::ptr::null_mut();
            }
            // SAFETY: the caller supplies count readable, aligned doubles.
            let values = unsafe { ::std::slice::from_raw_parts(values, count) };
            // SAFETY: the caller supplies len readable UTF-8 bytes.
            let bytes = unsafe { ::std::slice::from_raw_parts(text, len) };
            let Ok(text) = ::std::str::from_utf8(bytes) else {
                return ::std::ptr::null_mut();
            };
            $crate::calc::Context::new(text, values)
                .map_or(::std::ptr::null_mut(), |c| Box::into_raw(c).cast())
        }
        /// Resolve a style's calculation context.
        /// # Safety
        /// `context` is null or a live pointer returned by exact_calc_create.
        #[no_mangle]
        pub unsafe extern "C" fn exact_calc_resolve(
            context: *const ::std::ffi::c_void,
            basis: f64,
        ) -> f64 {
            // SAFETY: a non-null context is live and remains borrowed during this call.
            unsafe { context.cast::<$crate::calc::Context>().as_ref() }
                .map_or(0.0, |c| c.length(basis))
        }
        /// Release a style's calculation context.
        /// # Safety
        /// `context` is null or a live pointer from exact_calc_create, consumed once.
        #[no_mangle]
        pub unsafe extern "C" fn exact_calc_destroy(context: *mut ::std::ffi::c_void) {
            if !context.is_null() {
                // SAFETY: create allocated this Box and the caller transfers ownership.
                drop(unsafe { Box::from_raw(context.cast::<$crate::calc::Context>()) });
            }
        }
    };
}
#[cfg(test)]
mod tests {
    #[test]
    fn released_contexts_release_calculations() {
        use std::hash::{Hash, Hasher};
        let identity = |c: &exact_kernel::style::Comparison| {
            let mut hash = std::hash::DefaultHasher::new();
            c.hash(&mut hash);
            hash.finish()
        };
        // A serialized value owns its expression until a context takes its own copy.
        let mut json = String::new();
        let exact_kernel::Dimension::Compare(c) =
            exact_kernel::Dimension::parse_calc("min(17px, 53%)").unwrap()
        else {
            unreachable!()
        };
        super::write(&mut json, c, &exact_kernel::Env::default());
        let survivor = super::Context::new(
            "min(47px, 53%)",
            &[0., 0., 0., 0., 300., 600., 1., 1., 0., 0.],
        )
        .unwrap();
        for i in 0..16_000 {
            let text = format!("min({i}px, 52%)");
            let context =
                super::Context::new(&text, &[0., 0., 0., 0., 300., 600., 1., 1., 0., 0.]).unwrap();
            assert_eq!(context.length(100.), f64::from((i as f32).min(52.)));
            let old = identity(&context.calculation);
            drop(context);
            let exact_kernel::Dimension::Compare(c) =
                exact_kernel::Dimension::parse_calc(&text).unwrap()
            else {
                unreachable!()
            };
            assert_ne!(
                identity(&c),
                old,
                "a released context must leave no bridge owner"
            );
        }
        assert_eq!(survivor.length(100.), 47.);
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let context = super::Context::new(
            value["calc"].as_str().unwrap(),
            &[0., 0., 0., 0., 300., 600., 1., 1., 0., 0.],
        )
        .unwrap();
        drop(value);
        drop(json);
        assert_eq!(context.length(100.), 17.);
    }

    #[test]
    fn contexts_keep_their_own_environment_and_validate_the_wire() {
        let n = "min(50%, 10vw + env(safe-area-inset-top))";
        let first = super::Context::new(n, &[3., 0., 0., 0., 300., 600., 1., 1., 0., 0.]).unwrap();
        for width in 400..1400 {
            let other =
                super::Context::new(n, &[7., 0., 0., 0., width as f64, 600., 1., 1., 0., 0.])
                    .unwrap();
            assert!((other.length(400.) - (width as f64 / 10. + 7.).min(200.)).abs() < 0.0001);
            assert_eq!(first.length(400.), 33.);
        }
        assert!(super::Context::new("", &[0.; 9]).is_none());
        assert!(
            super::Context::new(n, &[0.; 8]).is_none(),
            "no region count"
        );
        for bad in [0., 1.5, 256., f64::INFINITY, f64::NAN] {
            assert!(
                super::Context::new(n, &[0., 0., 0., 0., 300., 600., bad, 1., 0., 0.]).is_none()
            );
        }
        assert!(super::Context::new(n, &[0., 0., 0., 0., 300., 600., 2., 1., 0., 0.]).is_none());
    }

    /// LLP 1123 D10: a deferred calculation reads the layout regions the
    /// snapshot carries, and the safe area without them.
    #[test]
    fn a_snapshot_carries_the_layout_regions() {
        let n = "min(50%, env(-exact-layout-margins-inset-left))";
        let mut env = exact_kernel::Env::new(0., 0., 0., 3.);
        env.viewport_width = 300.;
        env.viewport_height = 600.;
        let values = |env: &exact_kernel::Env| -> Vec<f64> {
            let exact_kernel::Dimension::Compare(c) = exact_kernel::Dimension::parse_calc(
                "min(50%, env(-exact-layout-margins-inset-left))",
            )
            .unwrap() else {
                panic!("expected comparison");
            };
            let mut json = String::new();
            super::write(&mut json, c.clone(), env);
            let value: serde_json::Value = serde_json::from_str(&json).unwrap();
            value["env"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap())
                .collect()
        };
        let none = super::Context::new(n, &values(&env)).unwrap();
        assert_eq!(none.length(100.), 3., "the safe area without regions");
        let mut regions = [[0f32; 4]; 5];
        regions[1][3] = 20.;
        let with = super::Context::new(n, &values(&env.with_regions(Some(regions)))).unwrap();
        assert_eq!(with.length(100.), 20.);
        assert_eq!(with.length(30.), 15.);
        let mut short = values(&env.with_regions(Some(regions)));
        short.remove(9);
        assert!(super::Context::new(n, &short).is_none(), "nineteen regions");
        for count in [-1., 20.5, 4.] {
            let mut bad = values(&env);
            bad[8] = count;
            assert!(super::Context::new(n, &bad).is_none(), "a count of {count}");
        }
    }

    /// A deferred calculation reads the title-bar area the snapshot
    /// carries, and its fallback where there is none.
    #[test]
    fn a_snapshot_carries_the_titlebar_area() {
        let text = "max(25%, calc(env(titlebar-area-x) + 4px))";
        let n = text;
        let exact_kernel::Dimension::Compare(c) =
            exact_kernel::Dimension::parse_calc(text).unwrap()
        else {
            panic!("expected comparison");
        };
        let values = |env: &exact_kernel::Env| -> Vec<f64> {
            let mut json = String::new();
            super::write(&mut json, c.clone(), env);
            let value: serde_json::Value = serde_json::from_str(&json).unwrap();
            value["env"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap())
                .collect()
        };
        let env = exact_kernel::Env::default();
        let area = env.with_titlebar_area(Some(exact_kernel::Rect::new(78., 0., 822., 32.)));
        let with = super::Context::new(n, &values(&area)).unwrap();
        assert_eq!(with.length(100.), 82.);
        assert_eq!(with.length(400.), 100.);
        // Undefined, without a fallback: no length, which the kernel never
        // sends, since the row takes its initial value first
        let none = super::Context::new(n, &values(&env)).unwrap();
        assert_eq!(none.length(100.), 0.);
        let mut three = values(&area);
        three.remove(10);
        assert!(super::Context::new(n, &three).is_none(), "three sides");
        let mut count = values(&area);
        count[9] = 3.;
        assert!(super::Context::new(n, &count).is_none(), "a count of 3");
    }

    #[test]
    fn resizing_does_not_intern_environment_snapshots() {
        let exact_kernel::Dimension::Compare(c) =
            exact_kernel::Dimension::parse_calc("min(23%, 17vw)").unwrap()
        else {
            panic!("expected comparison");
        };
        let mut env = exact_kernel::Env::default();
        let mut first = String::new();
        c.css(&mut first);
        for width in 1..1000 {
            env.viewport_width = width as f32;
            let mut json = String::new();
            super::write(&mut json, c.clone(), &env);
            let value: serde_json::Value = serde_json::from_str(&json).unwrap();
            assert_eq!(value["calc"], first);
        }
    }

    #[test]
    fn a_context_resolves_against_its_basis_as_the_kernel_does() {
        let n = "min(50%, 30px)";
        let context = super::Context::new(n, &[0., 0., 0., 0., 0., 0., 1., 1., 0., 0.]).unwrap();
        assert_eq!(context.length(40.0), 20.0);
        assert_eq!(context.length(200.0), 30.0);
        assert!(
            super::Context::new("10px", &[0., 0., 0., 0., 300., 600., 1., 1., 0., 0.]).is_none()
        );
    }
}

#[cfg(test)]
#[allow(unsafe_code)]
mod ffi_tests {
    extern "C" {
        fn exact_calc_create(
            text: *const u8,
            len: usize,
            values: *const f64,
            count: usize,
        ) -> *mut std::ffi::c_void;
        fn exact_calc_resolve(context: *const std::ffi::c_void, basis: f64) -> f64;
        fn exact_calc_destroy(context: *mut std::ffi::c_void);
    }

    #[test]
    fn the_native_entry_point_owns_its_expression_and_validates_utf8() {
        let text = b"min(47px, 53%)".to_vec();
        let values = [0., 0., 0., 0., 300., 600., 1., 1., 0., 0.];
        // SAFETY: both buffers remain readable for the call; create copies them.
        let context =
            unsafe { exact_calc_create(text.as_ptr(), text.len(), values.as_ptr(), values.len()) };
        assert!(!context.is_null());
        drop(text);
        // SAFETY: context is live until the single destroy below.
        unsafe {
            assert_eq!(exact_calc_resolve(context, 100.), 47.);
            exact_calc_destroy(context);
            let invalid = [0xff];
            assert!(
                exact_calc_create(invalid.as_ptr(), 1, values.as_ptr(), values.len()).is_null()
            );
            assert!(
                exact_calc_create(std::ptr::null(), 0, values.as_ptr(), values.len()).is_null()
            );
        }
    }
}
