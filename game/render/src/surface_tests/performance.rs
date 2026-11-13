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

#[test]
fn gpu_time_is_measured_only_for_a_reader_and_only_with_timestamps() {
    let Some(gpu) = gpu() else { return };
    let timestamps = gpu
        .device
        .features()
        .contains(wgpu::Features::TIMESTAMP_QUERY);
    let mut surface = surface();
    fixture::render(&gpu, &mut surface, &frame(0.)).unwrap();
    fixture::render(&gpu, &mut surface, &frame(167.)).unwrap();
    assert!(surface.gpu_timing.is_none(), "no reader, no timestamp work");
    surface.set_performance_reporting(true);
    for i in 2..8 {
        fixture::render(&gpu, &mut surface, &frame(i as f64 * 167.)).unwrap();
        // The frame finishes; the next frame's poll resolves and maps it.
        gpu.device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
    }
    assert_eq!(surface.gpu_timing.is_some(), timestamps);
    assert!(
        surface.gpu_timing.as_ref().is_none_or(|t| !t.armed()),
        "a reader's timing keeps no agent samples"
    );
    // At rest, the snapshot's own poll publishes the last mapped frame.
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    let report: serde_json::Value =
        serde_json::from_str(&surface.take_performance(0.).unwrap()).unwrap();
    if timestamps {
        let (mean, max) = (report["gpuMs"].as_f64(), report["gpuMaxMs"].as_f64());
        assert!(mean.is_some_and(|ms| ms > 0.) && max >= mean, "{report}");
    } else {
        assert!(report.get("gpuMs").is_none(), "no timestamp queries: none");
    }
    let state = surface.agent(r#"{"op":"state"}"#).unwrap();
    assert!(
        !state.contains("\"gpuMs\""),
        "a reader's timing is not the agent's"
    );
    surface.set_performance_reporting(false);
    assert!(
        surface.gpu_timing.is_none(),
        "the last reader gone, none left"
    );
    fixture::render(&gpu, &mut surface, &frame(9. * 167.)).unwrap();
    assert!(surface.gpu_timing.is_none());
    surface.set_performance_reporting(true);
    fixture::render(&gpu, &mut surface, &frame(10. * 167.)).unwrap();
    surface.agent(r#"{"op":"state","perf":true}"#).unwrap();
    assert!(
        surface.gpu_timing.is_none(),
        "arming perf drops a reader's sample-less timing at once"
    );
}
