// The painted exporter is shared with Android; metadata follows the host.
include!("../service.rs");

const PRESENT_METHOD: &str = "modeset";

fn platform_meta() -> Map<String, Value> {
    object(json!({
        "osType": "linux",
        "osName": "Linux",
        "osVersion": read("/proc/sys/kernel/osrelease"),
        "deviceModel": read("/sys/devices/virtual/dmi/id/product_name"),
        // `en_US.UTF-8` becomes `en-US`. `C` and `POSIX` mean no language.
        "language": std::env::var("LANG").ok().map(|l| l.split('.').next().unwrap_or("").replace('_', "-"))
            .filter(|l| !l.is_empty() && l != "C" && l != "POSIX"),
        "clientVersion": CLIENT_VERSION,
    }))
}

/// The device state Observe attaches to TTI, read when the startup report arrives.
/// Low-power mode comes from the ACPI platform profile. Missing hardware is left out.
fn device() -> Map<String, Value> {
    let mut d = Map::new();
    if let Some(profile) = read("/sys/firmware/acpi/platform_profile") {
        d.insert(
            "expo.device.lowPowerMode".into(),
            (profile == "low-power" || profile == "quiet").into(),
        );
    }
    for entry in std::fs::read_dir("/sys/class/power_supply")
        .into_iter()
        .flatten()
        .flatten()
    {
        let p = entry.path();
        if read(&format!("{}/type", p.display())).as_deref() != Some("Battery") {
            continue;
        }
        if let Some(c) =
            read(&format!("{}/capacity", p.display())).and_then(|c| c.parse::<f64>().ok())
        {
            d.insert("expo.device.batteryLevel".into(), json!(c / 100.0));
        }
        if let Some(s) = read(&format!("{}/status", p.display())) {
            d.insert(
                "expo.device.batteryCharging".into(),
                (s == "Charging" || s == "Full").into(),
            );
        }
        break;
    }
    let mut kind = None;
    for entry in std::fs::read_dir("/sys/class/net")
        .into_iter()
        .flatten()
        .flatten()
    {
        let p = entry.path();
        if entry.file_name() == "lo"
            || read(&format!("{}/operstate", p.display())).as_deref() != Some("up")
        {
            continue;
        }
        let wifi = p.join("wireless").exists();
        if kind.is_none() || wifi {
            kind = Some(if wifi {
                "wifi"
            } else if p.join("device").exists() {
                "ethernet"
            } else {
                "other"
            });
        }
    }
    d.insert("expo.network.connected".into(), kind.is_some().into());
    d.insert("expo.network.type".into(), kind.unwrap_or("none").into());
    d
}

fn build_identity() -> Option<String> {
    std::fs::metadata("/proc/self/exe").ok().map(|m| {
        let mtime = m.modified().ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok());
        format!("{}:{}", m.len(), mtime.map_or(0, |d| d.as_nanos()))
    })
}
