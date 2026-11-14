use super::*;
use serde_json::json;

fn shader_fixture() -> (std::path::PathBuf, Value) {
    let (path, mut compat) = super::super::tests::fixture();
    let source = path.with_file_name("probe.c");
    let mut text = std::fs::read_to_string(&source).unwrap().replace(
        r#"uint32_t gpu_shader_names(void) { output="[]"; return 2; }"#,
        "",
    );
    text.push_str(
        r#"
static char shader_calls[256];
static uint32_t shader_at;
static const char *shader_roster="[\"a\",\"b\"]";
uint32_t gpu_load(void) { return 0; }
static uint32_t readbacks;
uint32_t gpu_readback(uint32_t id, float w, float h, float scale, double now, unsigned char *pixels, size_t len) {
  readbacks++; memset(pixels, 0, len); return 0;
}
uint32_t test_readbacks(void) { return readbacks; }
uint32_t gpu_shader_names(void) { output=shader_roster; return strlen(output); }
void test_unknown_shader(void) { shader_roster="[\"a\",\"unexpected\"]"; }
uint32_t gpu_shader_validate(const unsigned char *n, size_t nl, const unsigned char *s, size_t sl) {
  shader_calls[shader_at++]='v';
  return nl!=1 || (n[0]!='a' && n[0]!='b') || (sl>=6 && !memcmp(s,"reject",6));
}
void gpu_shaders_clear(void) { shader_calls[shader_at++]='c'; }
uint32_t gpu_shader(const unsigned char *n, size_t nl, const unsigned char *s, size_t sl) {
  shader_calls[shader_at++]='s'; return sl>=4 && !memcmp(s,"fail",4);
}
const char *test_shader_calls(void) { return shader_calls; }
"#,
    );
    std::fs::write(&source, text).unwrap();
    super::super::tests::compile_fixture(&source, &path);
    compat["embedded"]["gpu"]["sha256"] =
        super::super::digest::bytes_digest(&std::fs::read(&path).unwrap(), true).into();
    compat["inputs"]["gpuSurfaces"] = json!([{"name":"a"},{"name":"b"}]);
    (path, compat)
}

fn selected(a: &'static [u8], b: &'static [u8]) -> Assets {
    Assets::selected(
        Default::default(),
        Arc::new(move |name| {
            Ok(match name {
                "shaders/a.wgsl" => Some(Arc::from(a)),
                "shaders/b.wgsl" => Some(Arc::from(b)),
                _ => None,
            })
        }),
    )
}

fn read_sources(compat: &Value, assets: &Assets) -> Result<Pack, String> {
    let names = compat["inputs"]["gpuSurfaces"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["name"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    read_pack(compat, assets, &names)
}

fn calls(abi: &Abi) -> String {
    unsafe {
        let ptr =
            abi.symbol::<unsafe extern "C" fn() -> *const std::ffi::c_char>(b"test_shader_calls")();
        std::ffi::CStr::from_ptr(ptr).to_str().unwrap().into()
    }
}

#[test]
fn native_device_registers_shaders_and_repaints_keep_the_native_surface() {
    use crate::presenter::PainterChoice;

    let (path, compat) = shader_fixture();
    let plan =
        contract::compile("component App\n  view\n    canvas width=100 height=100\n").unwrap();
    let (mut p, error) = crate::Presenter::boot_with(
        &plan.encode(),
        (),
        (100., 100.),
        1.,
        path.parent().unwrap().into(),
        PainterChoice::Canvas,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let view = p
        .host
        .preorder()
        .into_iter()
        .find(|id| p.host.kernel().node(*id).unwrap().node_type == exact_kernel::NodeType::Canvas)
        .unwrap();
    let mut abi = Abi::open_path(&path, &compat, "").unwrap();
    // Android loads a device for its reader's native target, without readback.
    abi.load_device().unwrap();
    p.surfaces.abis.insert(String::new(), abi);
    let packs = p
        .surfaces
        .prepare_shaders(&compat.to_string(), &selected(b"a", b"b"))
        .unwrap();
    assert_eq!(packs[""].len(), 2);
    p.surfaces.commit_shaders(packs).unwrap();
    assert_eq!(calls(&p.surfaces.abis[""]), "vvcss");
    p.surfaces.canvases.insert(
        view,
        super::super::Canvas {
            id: 1,
            name: "native".into(),
            artifact: String::new(),
            owner: true,
            performance: false,
            since: 0,
            held: Default::default(),
            restored_controls: None,
            restore_error: None,
            restore_input: false,
            restore_bytes: None,
            restore_logged: false,
            uploaded: None,
        },
    );
    for _ in 0..2 {
        p.repaint();
        p.frame();
        let ops = crate::canvas::FINISHED
            .with(|f| f.borrow_mut().take())
            .unwrap();
        assert!(
            ops.windows(3)
                .any(|op| op[0] == 28 && op[1] == view && op[2] & 0xff == 3),
            "a repaint must retain the native surface: {ops:?}"
        );
        assert!(p.brush.canvases.is_empty());
    }
    assert_eq!(
        unsafe { p.surfaces.abis[""].symbol::<unsafe extern "C" fn() -> u32>(b"test_readbacks")() },
        0
    );
    drop(p);
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn validates_complete_pack_before_replacement_and_unchanged_pack_does_no_gpu_work() {
    let (path, compat) = shader_fixture();
    let mut abi = Abi::open_path(&path, &compat, "").unwrap();
    abi.load_device().unwrap();
    abi.rendered = true;
    let pack = abi
        .prepare_shaders(&compat, &selected(b"old-a", b"old-b"))
        .unwrap();
    assert_eq!(calls(&abi), "vv");
    assert!(abi.shaders.is_empty());
    abi.commit_shaders(pack).unwrap();
    assert_eq!(calls(&abi), "vvcss");
    let old = abi.shaders.clone();
    assert!(abi
        .prepare_shaders(&compat, &selected(b"old-a", b"old-b"))
        .unwrap()
        .is_none());
    assert_eq!(calls(&abi), "vvcss");
    // The first candidate source is valid; the second refuses. No clear or
    // registration may occur, and the current complete pack stays live.
    assert!(abi
        .prepare_shaders(&compat, &selected(b"new-a", b"reject-interface"))
        .unwrap_err()
        .contains("`b`"));
    assert_eq!(calls(&abi), "vvcssvv");
    assert_eq!(abi.shaders, old);
    let pack = abi
        .prepare_shaders(&compat, &selected(b"new-a", b"new-b"))
        .unwrap();
    abi.commit_shaders(pack).unwrap();
    assert_eq!(calls(&abi), "vvcssvvvvcss");
    assert_eq!(&*abi.shaders["a"], b"new-a");
    // An integrity refusal is checked even when the accepted names are unchanged.
    let corrupt = Assets::selected(
        Default::default(),
        Arc::new(|_| Err("signed digest mismatch".into())),
    );
    assert!(abi
        .prepare_shaders(&compat, &corrupt)
        .unwrap_err()
        .contains("signed digest"));
    assert_eq!(calls(&abi), "vvcssvvvvcss");
    unsafe { abi.symbol::<unsafe extern "C" fn()>(b"test_unknown_shader")() };
    let extra = Assets::selected(
        Default::default(),
        Arc::new(|_| Ok(Some(Arc::from(b"source".as_slice())))),
    );
    assert!(abi
        .prepare_shaders(&compat, &extra)
        .unwrap_err()
        .contains("unexpected"));
    drop(abi);
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn headless_shaderful_module_needs_neither_sources_nor_shader_symbols() {
    let (path, mut compat) = super::super::tests::fixture();
    compat["inputs"]["gpuSurfaces"] = json!([{"name":"never-read"}]);
    let mut abi = Abi::open_path(&path, &compat, "").unwrap();
    let assets = Assets::selected(
        Default::default(),
        Arc::new(|_| panic!("headless must not read")),
    );
    assert!(abi.prepare_shaders(&compat, &assets).unwrap().is_none());
    abi.commit_shaders(None).unwrap();
    assert_eq!(
        unsafe {
            abi.symbol::<unsafe extern "C" fn(u32, *const u8, usize, f64) -> u32>(b"gpu_bind_at")(
                1,
                b"[]".as_ptr(),
                2,
                0.,
            )
        },
        0
    );
    let mut surfaces = Surfaces::default();
    surfaces.abis.insert(String::new(), abi);
    assert!(surfaces.prepare_shaders("", &assets).unwrap().is_empty());
    drop(surfaces);
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn shader_sources_enforce_roster_integrity_utf8_bounds_and_selected_tombstones() {
    let root = std::env::temp_dir().join(format!("exact-shader-files-{}", std::process::id()));
    std::fs::create_dir_all(root.join("shaders")).unwrap();
    let path = root.join("shaders/a.wgsl");
    std::fs::write(&path, b"valid").unwrap();
    let mut compat = json!({"inputs":{"gpuSurfaces":[{"name":"a"}]},"embedded":{"assets":[{
        "name":"shaders/a.wgsl","bytes":5,"sha256":format!("{:x}",Sha256::digest(b"valid"))
    }]}});
    let assets = Assets::embedded(root.clone());
    assert_eq!(&*read_sources(&compat, &assets).unwrap()["a"], b"valid");
    std::fs::write(&path, b"other").unwrap();
    assert!(read_sources(&compat, &assets)
        .unwrap_err()
        .contains("digest mismatch"));
    // Selected source has its own integrity authority, not entry zero's hash.
    assert_eq!(
        &*read_sources(&compat, &selected(b"changed", b"")).unwrap()["a"],
        b"changed"
    );
    let absent = Assets::selected(root.clone(), Arc::new(|_| Ok(None)));
    assert!(read_sources(&compat, &absent)
        .unwrap_err()
        .contains("missing source"));
    assert!(read_sources(&compat, &selected(b"\xff", b""))
        .unwrap_err()
        .contains("UTF-8"));
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(LIMIT as u64 + 1).unwrap();
    drop(file);
    assert!(read_sources(&compat, &assets)
        .unwrap_err()
        .contains("256 MiB"));
    compat["inputs"]["gpuSurfaces"] = json!([{"name":"a"},{"name":"a"}]);
    assert!(read_sources(&compat, &selected(b"a", b"b"))
        .unwrap_err()
        .contains("duplicate"));
    compat["inputs"]["gpuSurfaces"] = json!([{"name":"../a"}]);
    assert!(read_sources(&compat, &assets)
        .unwrap_err()
        .contains("invalid"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn shader_and_store_acceptance_roll_back_without_advancing_either_owner() {
    let (path, compat) = shader_fixture();
    let mut abi = Abi::open_path(&path, &compat, "").unwrap();
    abi.load_device().unwrap();
    let pack = abi
        .prepare_shaders(&compat, &selected(b"old-a", b"old-b"))
        .unwrap();
    abi.commit_shaders(pack).unwrap();
    let old = abi.shaders.clone();
    let mut surfaces = Surfaces::default();
    surfaces.abis.insert(String::new(), abi);
    let pack = surfaces
        .prepare_shaders(
            &compat.to_string(),
            &selected(b"new-a", b"fail-registration"),
        )
        .unwrap();
    assert!(surfaces
        .activate_shaders(pack, || panic!(
            "registration failure cannot reach the store"
        ))
        .is_err());
    assert_eq!(surfaces.abis[""].shaders, old);
    assert!(calls(&surfaces.abis[""]).ends_with("vvcsscss"));
    let pack = surfaces
        .prepare_shaders(&compat.to_string(), &selected(b"new-a", b"new-b"))
        .unwrap();
    assert_eq!(
        surfaces.activate_shaders(pack, || Err("store refused".into())),
        Err("store refused".into())
    );
    assert_eq!(surfaces.abis[""].shaders, old);
    assert!(calls(&surfaces.abis[""]).ends_with("vvcsscss"));
    drop(surfaces);
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn disjoint_module_shaders_are_registered_with_their_owner() {
    let (path, mut compat) = shader_fixture();
    let source = path.with_file_name("probe.c");
    let text = std::fs::read_to_string(&source).unwrap();
    let extra = path.with_file_name(format!("libextra.{}", std::env::consts::DLL_EXTENSION));
    for (library, name) in [(&path, "a"), (&extra, "b")] {
        let text = text
            .replace("(n[0]!='a' && n[0]!='b')", &format!("n[0]!='{name}'"))
            .replace(
                r#"shader_roster="[\"a\",\"b\"]"#,
                &format!(r#"shader_roster="[\"{name}\"]"#),
            );
        std::fs::write(&source, text).unwrap();
        super::super::tests::compile_fixture(&source, library);
    }
    compat["embedded"]["gpu"]["sha256"] =
        super::super::digest::bytes_digest(&std::fs::read(&path).unwrap(), true).into();
    let mut card = compat["embedded"]["gpu"].clone();
    card["name"] = extra.file_name().unwrap().to_str().unwrap().into();
    card["sha256"] =
        super::super::digest::bytes_digest(&std::fs::read(&extra).unwrap(), true).into();
    compat["embedded"]["gpuModules"] = json!({"extra":card});
    compat["inputs"]["gpuModules"] = json!({"extra":["secondary_surface"]});
    let mut surfaces = Surfaces::default();
    for (artifact, library) in [("", &path), ("extra", &extra)] {
        let mut abi = Abi::open_path(library, &compat, artifact).unwrap();
        abi.load_device().unwrap();
        surfaces.abis.insert(artifact.into(), abi);
    }
    let pack = surfaces.prepare_shaders(&compat.to_string(), &selected(b"old-a", b"old-b"));
    assert!(
        pack.is_ok(),
        "each module must receive only its own shaders: {pack:?}"
    );
    surfaces.commit_shaders(pack.unwrap()).unwrap();
    assert_eq!(&*surfaces.abis[""].shaders["a"], b"old-a");
    assert_eq!(&*surfaces.abis["extra"].shaders["b"], b"old-b");
    assert_eq!(surfaces.abis[""].shaders.len(), 1);
    assert_eq!(surfaces.abis["extra"].shaders.len(), 1);
    let old = surfaces.previous_shaders(
        &surfaces
            .prepare_shaders(&compat.to_string(), &selected(b"new-a", b"new-b"))
            .unwrap(),
    );
    // A refusal in the last module must precede any namespace replacement.
    assert!(surfaces
        .prepare_shaders(
            &compat.to_string(),
            &selected(b"new-a", b"reject-interface")
        )
        .is_err());
    assert_eq!(surfaces.abis[""].shaders, old[""]);
    assert_eq!(surfaces.abis["extra"].shaders, old["extra"]);
    for (assets, reason) in [
        (
            Assets::selected(Default::default(), Arc::new(|_| Ok(None))),
            "missing source",
        ),
        (
            Assets::selected(
                Default::default(),
                Arc::new(|_| Err("signed digest mismatch".into())),
            ),
            "digest",
        ),
        (selected(b"new-a", b"\xff"), "UTF-8"),
    ] {
        assert!(surfaces
            .prepare_shaders(&compat.to_string(), &assets)
            .unwrap_err()
            .contains(reason));
        assert_eq!(surfaces.abis[""].shaders, old[""]);
        assert_eq!(surfaces.abis["extra"].shaders, old["extra"]);
    }
    // Validation succeeds, but the second module refuses registration.
    let pack = surfaces
        .prepare_shaders(
            &compat.to_string(),
            &selected(b"new-a", b"fail-registration"),
        )
        .unwrap();
    assert!(surfaces
        .activate_shaders(pack, || panic!("cannot accept a partial update"))
        .is_err());
    assert_eq!(surfaces.abis[""].shaders, old[""]);
    assert_eq!(surfaces.abis["extra"].shaders, old["extra"]);
    assert!(calls(&surfaces.abis[""]).ends_with("cscs"));
    assert!(calls(&surfaces.abis["extra"]).ends_with("cscs"));
    let pack = surfaces
        .prepare_shaders(&compat.to_string(), &selected(b"new-a", b"new-b"))
        .unwrap();
    assert_eq!(
        surfaces.activate_shaders(pack, || Err("store refused".into())),
        Err("store refused".into())
    );
    assert_eq!(surfaces.abis[""].shaders, old[""]);
    assert_eq!(surfaces.abis["extra"].shaders, old["extra"]);
    let pack = surfaces
        .prepare_shaders(&compat.to_string(), &selected(b"new-a", b"new-b"))
        .unwrap();
    surfaces.activate_shaders(pack, || Ok(())).unwrap();
    assert_eq!(&*surfaces.abis[""].shaders["a"], b"new-a");
    assert_eq!(&*surfaces.abis["extra"].shaders["b"], b"new-b");
    assert!(surfaces
        .prepare_shaders(&compat.to_string(), &selected(b"new-a", b"new-b"))
        .unwrap()
        .is_empty());
    surfaces.abis.remove("");
    let assets = Assets::selected(
        Default::default(),
        Arc::new(|name| {
            assert_eq!(name, "shaders/b.wgsl");
            Ok(Some(Arc::from(b"extra-only".as_slice())))
        }),
    );
    let pack = surfaces
        .prepare_shaders(&compat.to_string(), &assets)
        .unwrap();
    surfaces.commit_shaders(pack).unwrap();
    assert_eq!(&*surfaces.abis["extra"].shaders["b"], b"extra-only");
    drop(surfaces);
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}
