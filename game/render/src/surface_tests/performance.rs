use super::*;

#[test]
fn contract_performance_is_opt_in_and_does_not_change_simulation() {
    let Some(gpu) = gpu() else { return };
    let mut surface = surface();
    fixture::render(&gpu, &mut surface, &frame(0.)).unwrap();
    assert!(surface.take_performance(0.).is_none());
    let saved = surface.sim().unwrap().save().unwrap();
    let hash = surface.sim().unwrap().world().hash();
    surface.set_performance_reporting(true);
    assert!(surface.take_performance(0.).is_none());
    fixture::render(&gpu, &mut surface, &frame(0.)).unwrap();
    let report: serde_json::Value =
        serde_json::from_str(&surface.take_performance(0.).unwrap()).unwrap();
    assert_eq!(report["frames"], 1);
    assert_eq!(report["pixelWidth"], 64);
    assert_eq!(report["pixelHeight"], 64);
    assert!(report["draws"].as_u64().unwrap() > 0);
    assert!(report["geometryMemoryBytes"].as_u64().unwrap() > 0);
    assert!(report.get("textureMemoryBytes").is_some());
    assert!(
        report.get("thermalState").is_none(),
        "the fixture's host says none"
    );
    assert!(
        report.get("audioState").is_none(),
        "the fixture links no audio"
    );
    assert!(report.get("gpuMs").is_none());
    assert!(report.get("fps").is_none());
    assert!(report.get("windowMs").is_none());
    assert!(
        report.get("lateFrames").is_none(),
        "virtual time has no cadence"
    );
    assert_eq!(surface.sim().unwrap().save().unwrap(), saved);
    assert_eq!(surface.sim().unwrap().world().hash(), hash);
    surface
        .bind(&[Value::Bool(true), Value::Number(1.)], None)
        .unwrap();
    fixture::render(&gpu, &mut surface, &frame(0.)).unwrap();
    assert!(
        surface.take_performance(0.).is_some(),
        "new setup retains opt-in"
    );
    let carry = surface.carry().unwrap().unwrap();
    surface.restore(&carry, exact_gpu::Restore::Carry).unwrap();
    fixture::render(&gpu, &mut surface, &frame(0.)).unwrap();
    assert!(
        surface.take_performance(0.).is_some(),
        "restore retains opt-in"
    );
    surface.set_performance_reporting(false);
    fixture::render(&gpu, &mut surface, &frame(0.)).unwrap();
    assert!(surface.take_performance(0.).is_none());
}
