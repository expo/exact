//! A design's `AccentColor` seeds Material's scheme in place of the
//! device's palette (LLP 1115.001 D4), a binary of its own: the reported
//! table is the process's.
use exact_painted::presenter::{material, Material, PainterChoice, Palette, MATERIAL_ROLES};
use exact_painted::Presenter;

const VIEW: &str = r#"component App
  view
    column width=400 height=600 align-items="flex-start"
      button appearance="auto" buttonStyle="filled" width=120 height=40 testId="filled"
        text "Send"
"#;

/// The filled button's pixels, row by row.
fn button(p: &mut Presenter<()>) -> Vec<[u8; 3]> {
    let k = p.host().kernel();
    let f = k
        .node(k.node_by_key(k.find_by_test_id("filled")[0]).unwrap().id)
        .unwrap()
        .frame;
    let shot = p.frame();
    (0..f.height as u32)
        .flat_map(|y| (0..f.width as u32).map(move |x| (x, y)))
        .map(|(x, y)| {
            let c = shot.pixel(f.x as u32 + x, f.y as u32 + y).unwrap();
            [c.red(), c.green(), c.blue()]
        })
        .collect()
}

/// The app booted with `theme` as its design's one theme line.
fn boot(theme: &str) -> Presenter<()> {
    let dir = std::env::temp_dir().join(format!("exact-design-accent-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("app.forma"),
        format!("forma t\n\ntheme\n  {theme}\n"),
    )
    .unwrap();
    std::fs::write(dir.join("app.contract"), VIEW).unwrap();
    let plan = contract::compile_path(&dir.join("app.contract")).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let (p, error) = Presenter::boot_with(
        &plan.encode(),
        (),
        (400., 600.),
        1.,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p
}

/// The device's palette: one colour for every role.
const DEVICE: Palette = Palette {
    light: [0xff11_2233; MATERIAL_ROLES],
    dark: [0xff11_2233; MATERIAL_ROLES],
};

#[test]
fn a_designs_accent_seeds_the_scheme_and_paints_the_controls() {
    let mut p = boot("AccentColor = light-dark(#245c46, #8cd5b4)");
    p.set_palette(&DEVICE);
    // Google's Material Color Utilities, SchemeTonalSpot(#245c46), standard contrast.
    assert_eq!(
        material(Material::Primary, false),
        Some([0x1e, 0x6a, 0x4f, 0xff])
    );
    assert_ne!(
        material(Material::Surface, false),
        Some([0x11, 0x22, 0x33, 0xff])
    );
    // The control is the design's accent itself, with the ink that reads on it.
    let light = button(&mut p);
    assert_eq!(light[20 * 120 + 6], [0x24, 0x5c, 0x46]);
    assert!(light.contains(&[0xff; 3]), "white reads on the dark accent");
    p.set_system_scheme(true);
    let dark = button(&mut p);
    assert_eq!(dark[20 * 120 + 6], [0x8c, 0xd5, 0xb4]);
    assert!(
        !dark.contains(&[0xff; 3]),
        "white does not read on the light one"
    );

    // A role the palette maps is no seed: the device's palette stays.
    let mut p = boot("AccentColor = -exact-system-blue");
    p.set_palette(&DEVICE);
    assert_eq!(
        material(Material::Surface, false),
        Some([0x11, 0x22, 0x33, 0xff])
    );
}
