//! @exact/usb on the Linux host (LLP 1133.037 D13, D14, D21): usbfs. The devices and their configurations come
//! from sysfs (`/sys/bus/usb/devices`); `open` opens the device's node, `/dev/bus/usb/BBB/DDD`, once per process and
//! leases it to each connection (D5); claims, configurations, alternates, resets and transfers are usbfs's ioctls
//! (https://docs.kernel.org/driver-api/usb/usb.html).
//!
//! - Never `USBDEVFS_DISCONNECT`: an interface a kernel driver holds fails `NetworkError` at claim (`EBUSY`).
//! - WebUSB's protected interface classes cannot be claimed (`SecurityError`, D6).
//! - A stall (`EPIPE`) answers `stall`; a node udev left unreadable reads `granted` false and fails
//!   `NotAllowedError` at open (D17).
//! - Not built yet: transfers are the synchronous `USBDEVFS_CONTROL` and `USBDEVFS_BULK` on the loop's thread
//!   (five seconds at most), where the text has URBs reaped by a thread with `USBDEVFS_DISCARDURB` for a call let go.
//! - The painted presenter draws no chooser (D2): `requestDevice` fails `NotSupportedError`.
//!
//! The package names only `exact_painted` and `std`, so the ioctls and their structures are declared here.

use exact_painted::package::{json, Context, Json, Package, Reply};
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::ErrorKind;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

extern "C" {
    fn ioctl(fd: i32, request: u64, ...) -> i32;
}

const fn ioc(dir: u64, nr: u64, size: u64) -> u64 {
    (dir << 30) | (size << 16) | ((b'U' as u64) << 8) | nr
}
#[repr(C)]
struct CtrlTransfer {
    request_type: u8,
    request: u8,
    value: u16,
    index: u16,
    length: u16,
    timeout: u32,
    data: *mut u8,
}
#[repr(C)]
struct BulkTransfer {
    endpoint: u32,
    length: u32,
    timeout: u32,
    data: *mut u8,
}
#[repr(C)]
struct SetInterface {
    interface: u32,
    alternate: u32,
}
const CONTROL: u64 = ioc(3, 0, std::mem::size_of::<CtrlTransfer>() as u64);
const BULK: u64 = ioc(3, 2, std::mem::size_of::<BulkTransfer>() as u64);
const SETINTERFACE: u64 = ioc(2, 4, 8);
const SETCONFIGURATION: u64 = ioc(2, 5, 4);
const CLAIMINTERFACE: u64 = ioc(2, 15, 4);
const RELEASEINTERFACE: u64 = ioc(2, 16, 4);
const RESET: u64 = ioc(0, 20, 0);
const CLEAR_HALT: u64 = ioc(2, 21, 4);
const EPIPE: i32 = 32;
const EBUSY: i32 = 16;
/// WebUSB's protected interface classes (https://wicg.github.io/webusb/#protected-interface-classes).
const PROTECTED: [u64; 8] = [0x01, 0x03, 0x08, 0x09, 0x0B, 0x0E, 0x10, 0xE0];

fn text(p: &Path) -> Option<String> {
    std::fs::read_to_string(p).ok().map(|s| s.trim().to_owned())
}
fn hex(p: &Path) -> u64 {
    text(p).and_then(|v| u64::from_str_radix(&v, 16).ok()).unwrap_or(0)
}
fn dec(p: &Path) -> u64 {
    text(p).and_then(|v| v.parse().ok()).unwrap_or(0)
}
/// The device's `bcdUSB`, from sysfs's `version`, which prints it as `"%2x.%02x"` (` 2.10` is 0x0210): sysfs has no
/// `bcdUSB` file (`version_show`, drivers/usb/core/sysfs.c).
fn bcd_usb(p: &Path) -> u64 {
    let v = text(&p.join("version")).unwrap_or_default();
    let (major, minor) = v.split_once('.').unwrap_or_default();
    let field = |f: &str| u64::from_str_radix(f, 16).unwrap_or(0) & 0xFF;
    field(major) << 8 | field(minor)
}
/// The device's active configuration, none while it is unconfigured (`bConfigurationValue` 0 or empty).
fn configured(p: &Path) -> Option<u64> {
    Some(dec(&p.join("bConfigurationValue"))).filter(|&v| v > 0)
}

#[derive(Clone)]
struct Device {
    id: String,
    node: String,
    /// Its sysfs directory, `/sys/bus/usb/devices/1-2`.
    sys: PathBuf,
    record: Json,
}

/// Each interface directory of a device (`1-2:1.0`): its number, alternate, class and endpoints.
fn interfaces(dir: &Path) -> Vec<Json> {
    let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut by: BTreeMap<u64, Vec<Json>> = BTreeMap::new();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if !e.file_name().to_string_lossy().starts_with(&format!("{name}:")) {
            continue;
        }
        let endpoints: Vec<Json> = (std::fs::read_dir(&p).into_iter().flatten().flatten())
            .filter(|ep| ep.file_name().to_string_lossy().starts_with("ep_"))
            .filter_map(|ep| {
                let address = hex(&ep.path().join("bEndpointAddress"));
                let kind = match text(&ep.path().join("type"))?.as_str() {
                    "Bulk" => "bulk",
                    "Interrupt" => "interrupt",
                    "Isoc" => "isochronous",
                    _ => return None,
                };
                Some(json!({ "endpointNumber": address & 0x0F, "direction": if address & 0x80 != 0 { "in" } else { "out" }, "type": kind, "packetSize": hex(&ep.path().join("wMaxPacketSize")) }))
            })
            .collect();
        let alternate = json!({
            "alternateSetting": dec(&p.join("bAlternateSetting")), "interfaceClass": hex(&p.join("bInterfaceClass")),
            "interfaceSubclass": hex(&p.join("bInterfaceSubClass")), "interfaceProtocol": hex(&p.join("bInterfaceProtocol")),
            "interfaceName": text(&p.join("interface")), "endpoints": endpoints,
        });
        by.entry(hex(&p.join("bInterfaceNumber"))).or_default().push(alternate);
    }
    by.into_iter().map(|(n, alternates)| json!({ "interfaceNumber": n, "alternates": alternates })).collect()
}

fn devices() -> Vec<Device> {
    let mut out = Vec::new();
    for e in std::fs::read_dir("/sys/bus/usb/devices").into_iter().flatten().flatten() {
        let p: PathBuf = e.path();
        if !p.join("idVendor").exists() {
            continue;
        }
        let (bus, num) = (dec(&p.join("busnum")), dec(&p.join("devnum")));
        let (usb, bcd) = (bcd_usb(&p), hex(&p.join("bcdDevice")));
        let configuration = configured(&p);
        let id = format!("{:04x}:{:04x}@{}", hex(&p.join("idVendor")), hex(&p.join("idProduct")), e.file_name().to_string_lossy());
        let node = format!("/dev/bus/usb/{bus:03}/{num:03}");
        let granted = OpenOptions::new().read(true).write(true).open(&node).map_or_else(|e| e.kind() != ErrorKind::PermissionDenied, |_| true);
        let record = json!({
            "id": id, "vendorId": hex(&p.join("idVendor")), "productId": hex(&p.join("idProduct")),
            "usbVersionMajor": usb >> 8, "usbVersionMinor": (usb >> 4) & 0xF, "usbVersionSubminor": usb & 0xF,
            "deviceClass": hex(&p.join("bDeviceClass")), "deviceSubclass": hex(&p.join("bDeviceSubClass")), "deviceProtocol": hex(&p.join("bDeviceProtocol")),
            "deviceVersionMajor": bcd >> 8, "deviceVersionMinor": (bcd >> 4) & 0xF, "deviceVersionSubminor": bcd & 0xF,
            "manufacturerName": text(&p.join("manufacturer")).unwrap_or_default(), "productName": text(&p.join("product")).unwrap_or_default(),
            "serialNumber": text(&p.join("serial")), "configuration": configuration,
            "configurations": [{ "configurationValue": configuration.unwrap_or(1), "configurationName": text(&p.join("configuration")), "interfaces": interfaces(&p) }],
            "connected": true, "granted": granted,
        });
        out.push(Device { id, node, sys: p, record });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

fn classes(r: &Json) -> Vec<(u64, u64, u64)> {
    let mut out = vec![(r["deviceClass"].as_u64().unwrap_or(0), r["deviceSubclass"].as_u64().unwrap_or(0), r["deviceProtocol"].as_u64().unwrap_or(0))];
    for i in r["configurations"][0]["interfaces"].as_array().into_iter().flatten() {
        for a in i["alternates"].as_array().into_iter().flatten() {
            out.push((a["interfaceClass"].as_u64().unwrap_or(0), a["interfaceSubclass"].as_u64().unwrap_or(0), a["interfaceProtocol"].as_u64().unwrap_or(0)));
        }
    }
    out
}

fn matches(r: &Json, filters: &Json) -> bool {
    (filters.as_array().into_iter().flatten()).any(|f| {
        let eq = |k: &str, v: &Json| f[k].as_u64().is_none_or(|x| Some(x) == v.as_u64());
        eq("vendorId", &r["vendorId"])
            && eq("productId", &r["productId"])
            && f["serialNumber"].as_str().is_none_or(|s| r["serialNumber"] == s)
            && classes(r).iter().any(|(c, s, p)| f["classCode"].as_u64().is_none_or(|x| x == *c) && f["subclassCode"].as_u64().is_none_or(|x| x == *s) && f["protocolCode"].as_u64().is_none_or(|x| x == *p))
    })
}

struct Open {
    file: File,
    leases: usize,
    claims: BTreeMap<u32, String>,
}

struct Connection {
    device: Device,
    open: bool,
}

pub struct Usb {
    context: Context,
    opens: BTreeMap<String, Open>,
    connections: BTreeMap<String, Connection>,
    next: u64,
    remembered: Vec<String>,
}

fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            out.push(if i <= c.len() { T[(n >> shift) as usize & 63] as char } else { '=' });
        }
    }
    out
}

fn unbase64(text: &str) -> Option<Vec<u8>> {
    let (mut out, mut acc, mut bits) = (Vec::new(), 0u32, 0);
    for b in text.trim_end_matches('=').bytes() {
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32;
        acc = acc << 6 | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

fn failed() -> Json {
    json!({ "error": "NetworkError", "message": std::io::Error::last_os_error().to_string() })
}

impl Usb {
    fn handle(&self, args: &Json) -> Result<(String, &Connection, &Open), Json> {
        let h = args["connection"].as_str().unwrap_or_default();
        let c = self.connections.get(h).filter(|c| c.open).ok_or_else(|| json!({ "error": "InvalidStateError" }))?;
        let o = self.opens.get(&c.device.id).ok_or_else(|| json!({ "error": "NetworkError" }))?;
        Ok((h.to_owned(), c, o))
    }

    fn claimed(&self, args: &Json, n: u32) -> bool {
        self.handle(args).is_ok_and(|(h, _, o)| o.claims.get(&n) == Some(&h))
    }

    fn control(&self, args: &Json, incoming: bool) -> Json {
        let (_, _, o) = match self.handle(args) {
            Ok(found) => found,
            Err(e) => return e,
        };
        let setup = &args["setup"];
        let index = setup["index"].as_u64().unwrap_or(0) as u16;
        if setup["recipient"] == "interface" && !self.claimed(args, (index & 0xFF) as u32) {
            return json!({ "error": "InvalidStateError", "message": "the interface is not claimed" });
        }
        let kind = match setup["requestType"].as_str() { Some("class") => 1, Some("vendor") => 2, _ => 0 };
        let recipient = match setup["recipient"].as_str() { Some("interface") => 1, Some("endpoint") => 2, Some("other") => 3, _ => 0 };
        let mut data = match buffer(args, incoming, u16::MAX as u64, "control transfer") {
            Ok(data) => data,
            Err(e) => return e,
        };
        let mut t = CtrlTransfer {
            request_type: (if incoming { 0x80 } else { 0 }) | kind << 5 | recipient,
            request: setup["request"].as_u64().unwrap_or(0) as u8,
            value: setup["value"].as_u64().unwrap_or(0) as u16,
            index,
            length: data.len() as u16,
            timeout: 5000,
            data: data.as_mut_ptr(),
        };
        // SAFETY: `t` points at `data`, which outlives the call and holds `length` bytes.
        let n = unsafe { ioctl(o.file.as_raw_fd(), CONTROL, &mut t as *mut CtrlTransfer) };
        transferred(n, incoming, &data)
    }

    fn bulk(&self, args: &Json, incoming: bool) -> Json {
        let (_, _, o) = match self.handle(args) {
            Ok(found) => found,
            Err(e) => return e,
        };
        if o.claims.is_empty() {
            return json!({ "error": "InvalidStateError", "message": "no interface is claimed" });
        }
        let mut data = match buffer(args, incoming, bulk_most(), "transfer") {
            Ok(data) => data,
            Err(e) => return e,
        };
        let endpoint = args["endpointNumber"].as_u64().unwrap_or(0) as u32 | if incoming { 0x80 } else { 0 };
        let mut t = BulkTransfer { endpoint, length: data.len() as u32, timeout: 5000, data: data.as_mut_ptr() };
        // SAFETY: as for a control transfer.
        let n = unsafe { ioctl(o.file.as_raw_fd(), BULK, &mut t as *mut BulkTransfer) };
        transferred(n, incoming, &data)
    }

    fn uint(&self, args: &Json, request: u64, value: u32) -> Json {
        let (_, _, o) = match self.handle(args) {
            Ok(found) => found,
            Err(e) => return e,
        };
        let v = value;
        // SAFETY: these ioctls read one unsigned int through the pointer.
        if unsafe { ioctl(o.file.as_raw_fd(), request, &v as *const u32) } < 0 { failed() } else { Json::Null }
    }

    fn release_lease(&mut self, handle: &str) -> Json {
        let Some(c) = self.connections.get_mut(handle).filter(|c| c.open) else {
            return json!({ "error": "InvalidStateError" });
        };
        c.open = false;
        let id = c.device.id.clone();
        if let Some(o) = self.opens.get_mut(&id) {
            let mine: Vec<u32> = o.claims.iter().filter(|(_, h)| h.as_str() == handle).map(|(n, _)| *n).collect();
            for n in mine {
                // SAFETY: as in `uint`.
                unsafe { ioctl(o.file.as_raw_fd(), RELEASEINTERFACE, &n as *const u32) };
                o.claims.remove(&n);
            }
            o.leases -= 1;
            if o.leases == 0 {
                self.opens.remove(&id);
            }
        }
        self.context.changed("usb.usbConnection");
        Json::Null
    }
}

/// A transfer's buffer: `length` bytes to read, or the bytes to write, at most `most` either way; else the call's
/// `TypeError`, before anything the size of `length` is allocated. A control transfer's `wLength` is 16 bits, so its
/// `most` is 65,535, as macOS has it.
fn buffer(args: &Json, incoming: bool, most: u64, what: &str) -> Result<Vec<u8>, Json> {
    if incoming {
        return match args["length"].as_f64() {
            Some(n) if n >= 0.0 && n.fract() == 0.0 && n <= most as f64 => Ok(vec![0u8; n as usize]),
            _ => Err(json!({ "error": "TypeError", "message": format!("{what} length must be an integer from 0 to {most}") })),
        };
    }
    let data = args["data"].as_str().and_then(unbase64).unwrap_or_default();
    if data.len() as u64 > most {
        return Err(json!({ "error": "TypeError", "message": format!("{what} data must contain at most {most} bytes") }));
    }
    Ok(data)
}

/// The longest bulk or interrupt transfer usbfs takes: the memory its transfers may hold at once,
/// `usbcore.usbfs_memory_mb` (16 MB by default; https://docs.kernel.org/admin-guide/kernel-parameters.html), past
/// which the ioctl fails `ENOMEM`. Its 0, no limit, reads as a gibibyte: the ioctl's buffer is one allocation.
fn bulk_most() -> u64 {
    let mb = std::fs::read_to_string("/sys/module/usbcore/parameters/usbfs_memory_mb")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(16);
    if mb == 0 { 1 << 30 } else { (mb << 20).min(1 << 30) }
}

fn transferred(n: i32, incoming: bool, data: &[u8]) -> Json {
    match (n, std::io::Error::last_os_error().raw_os_error()) {
        (n, _) if n >= 0 && incoming => json!({ "status": "ok", "data": base64(&data[..n as usize]) }),
        (n, _) if n >= 0 => json!({ "status": "ok", "bytesWritten": n }),
        (_, Some(EPIPE)) if incoming => json!({ "status": "stall", "data": "" }),
        (_, Some(EPIPE)) => json!({ "status": "stall", "bytesWritten": 0 }),
        _ => failed(),
    }
}

impl Package for Usb {
    fn new(context: Context) -> Self {
        Usb { context, opens: BTreeMap::new(), connections: BTreeMap::new(), next: 0, remembered: Vec::new() }
    }

    fn later(&mut self, name: &str, args: &Json, _preferred: Option<&Json>, reply: Reply) {
        let answer = match name {
            "usbStatus" => json!({ "available": true, "attachedList": true, "state": "granted" }),
            "attachedDevices" => json!(devices().into_iter().filter(|d| matches(&d.record, &args["filters"])).map(|d| d.record).collect::<Vec<_>>()),
            "getDevices" => {
                let here = devices();
                json!(self.remembered.iter().map(|id| match here.iter().find(|d| &d.id == id) {
                    Some(d) => d.record.clone(),
                    None => json!({ "id": id, "vendorId": 0, "productId": 0, "usbVersionMajor": 0, "usbVersionMinor": 0, "usbVersionSubminor": 0, "deviceClass": 0, "deviceSubclass": 0, "deviceProtocol": 0, "deviceVersionMajor": 0, "deviceVersionMinor": 0, "deviceVersionSubminor": 0, "manufacturerName": "", "productName": "", "serialNumber": null, "configuration": null, "configurations": [], "connected": false, "granted": true }),
                }).collect::<Vec<_>>())
            }
            "requestDevice" => json!({ "error": "NotSupportedError" }),
            "open" => {
                let id = args["device"].as_str().unwrap_or_default().to_owned();
                match devices().into_iter().find(|d| d.id == id) {
                    None => json!({ "error": "NotFoundError" }),
                    Some(device) => {
                        let opened = match self.opens.get_mut(&id) {
                            Some(o) => Ok(o),
                            None => match OpenOptions::new().read(true).write(true).open(&device.node) {
                                Ok(file) => Ok(self.opens.entry(id.clone()).or_insert(Open { file, leases: 0, claims: BTreeMap::new() })),
                                Err(e) if e.kind() == ErrorKind::PermissionDenied => Err(json!({ "error": "NotAllowedError", "message": e.to_string() })),
                                Err(e) => Err(json!({ "error": "NetworkError", "message": e.to_string() })),
                            },
                        };
                        match opened {
                            Err(e) => e,
                            Ok(o) => {
                                o.leases += 1;
                                self.next += 1;
                                let handle = format!("usb-{}", self.next);
                                if !self.remembered.contains(&id) {
                                    self.remembered.push(id);
                                }
                                let answer = json!({ "connection": handle, "device": device.record });
                                self.connections.insert(handle, Connection { device, open: true });
                                self.context.changed("usb.getDevices");
                                answer
                            }
                        }
                    }
                }
            }
            "usbConnection" => match self.handle(args) {
                Ok((h, c, o)) => json!({ "open": true, "connected": true, "configuration": configured(&c.device.sys), "claimed": o.claims.iter().filter(|(_, c)| **c == h).map(|(n, _)| *n).collect::<Vec<_>>() }),
                Err(_) => json!({ "open": false, "connected": false, "configuration": null, "claimed": [] }),
            },
            "selectConfiguration" => self.uint(args, SETCONFIGURATION, args["configurationValue"].as_u64().unwrap_or(1) as u32),
            "claimInterface" => {
                let n = args["interfaceNumber"].as_u64().unwrap_or(0) as u32;
                match self.handle(args) {
                    Err(e) => e,
                    Ok((h, c, o)) => {
                        let class = (c.device.record["configurations"][0]["interfaces"].as_array().into_iter().flatten())
                            .find(|i| i["interfaceNumber"] == n)
                            .is_some_and(|i| (i["alternates"].as_array().into_iter().flatten()).any(|a| PROTECTED.contains(&a["interfaceClass"].as_u64().unwrap_or(0))));
                        if class {
                            json!({ "error": "SecurityError" })
                        } else if o.claims.get(&n).is_some_and(|other| *other != h) {
                            json!({ "error": "NetworkError", "message": "claimed by another connection" })
                        } else {
                            let fd = o.file.as_raw_fd();
                            // SAFETY: as in `uint`.
                            if unsafe { ioctl(fd, CLAIMINTERFACE, &n as *const u32) } < 0 {
                                let busy = std::io::Error::last_os_error().raw_os_error() == Some(EBUSY);
                                json!({ "error": "NetworkError", "message": if busy { "a kernel driver holds the interface".to_owned() } else { std::io::Error::last_os_error().to_string() } })
                            } else {
                                let id = c.device.id.clone();
                                self.opens.get_mut(&id).expect("open").claims.insert(n, h);
                                self.context.changed("usb.usbConnection");
                                Json::Null
                            }
                        }
                    }
                }
            }
            "releaseInterface" => {
                let n = args["interfaceNumber"].as_u64().unwrap_or(0) as u32;
                if !self.claimed(args, n) {
                    json!({ "error": "InvalidStateError" })
                } else {
                    let answer = self.uint(args, RELEASEINTERFACE, n);
                    if let Ok((_, c, _)) = self.handle(args) {
                        let id = c.device.id.clone();
                        self.opens.get_mut(&id).expect("open").claims.remove(&n);
                    }
                    answer
                }
            }
            "selectAlternateInterface" => {
                let n = args["interfaceNumber"].as_u64().unwrap_or(0) as u32;
                match self.handle(args) {
                    Err(e) => e,
                    Ok(_) if !self.claimed(args, n) => json!({ "error": "InvalidStateError" }),
                    Ok((_, _, o)) => {
                        let s = SetInterface { interface: n, alternate: args["alternateSetting"].as_u64().unwrap_or(0) as u32 };
                        // SAFETY: USBDEVFS_SETINTERFACE reads the struct through the pointer.
                        if unsafe { ioctl(o.file.as_raw_fd(), SETINTERFACE, &s as *const SetInterface) } < 0 { failed() } else { Json::Null }
                    }
                }
            }
            "controlTransferIn" => self.control(args, true),
            "controlTransferOut" => self.control(args, false),
            "transferIn" => self.bulk(args, true),
            "transferOut" => self.bulk(args, false),
            "clearHalt" => {
                let endpoint = args["endpointNumber"].as_u64().unwrap_or(0) as u32 | if args["direction"] == "in" { 0x80 } else { 0 };
                self.uint(args, CLEAR_HALT, endpoint)
            }
            "reset" => match self.handle(args) {
                Err(e) => e,
                // SAFETY: no argument.
                Ok((_, _, o)) => if unsafe { ioctl(o.file.as_raw_fd(), RESET) } < 0 { failed() } else { Json::Null },
            },
            "close" => self.release_lease(args["connection"].as_str().unwrap_or_default()),
            "forget" => {
                let id = args["device"].as_str().unwrap_or_default().to_owned();
                self.remembered.retain(|r| *r != id);
                let handles: Vec<String> = self.connections.iter().filter(|(_, c)| c.device.id == id && c.open).map(|(h, _)| h.clone()).collect();
                for h in handles {
                    self.release_lease(&h);
                }
                self.context.changed("usb.getDevices");
                Json::Null
            }
            _ => json!({ "error": "NotSupportedError" }),
        };
        reply.send(if answer.is_null() { json!({}) } else { answer });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_usb_version_and_configuration_are_read_as_sysfs_writes_them() {
        let dir = std::env::temp_dir().join(format!("exact-usb-sysfs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let read = |version: &str, configuration: &str| {
            std::fs::write(dir.join("version"), version).unwrap();
            std::fs::write(dir.join("bConfigurationValue"), configuration).unwrap();
            (bcd_usb(&dir), configured(&dir))
        };
        assert_eq!(read(" 2.00\n", "1\n"), (0x0200, Some(1)));
        assert_eq!(read(" 2.10\n", "2\n"), (0x0210, Some(2)));
        assert_eq!(read(" 1.10\n", "\n"), (0x0110, None), "a device with no active configuration");
        assert_eq!(read(" 3.20\n", "0\n"), (0x0320, None));
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!((bcd_usb(&dir), configured(&dir)), (0, None));
    }

    /// A control transfer's `wLength` is 16 bits: 65,536 would ask the device for 0 bytes, so it is refused, as a
    /// length past a bulk transfer's bound is, before a buffer that long is allocated.
    #[test]
    fn a_transfer_longer_than_its_ioctl_takes_is_refused_before_allocating() {
        let read = |length: Json, most: u64| buffer(&json!({ "length": length }), true, most, "transfer").map(|b| b.len());
        assert_eq!(read(json!(0), 65535), Ok(0));
        assert_eq!(read(json!(65535), 65535), Ok(65535));
        assert_eq!(read(json!(64.0), 65535), Ok(64));
        for refused in [json!(65536), json!(1u64 << 40), json!(-1), json!(1.5), json!(null), json!("8")] {
            assert_eq!(read(refused.clone(), 65535).unwrap_err()["error"], "TypeError", "{refused}");
        }
        assert_eq!(read(json!(u64::MAX), bulk_most()).unwrap_err()["error"], "TypeError");
        let write = |bytes: usize| buffer(&json!({ "data": base64(&vec![7u8; bytes]) }), false, 65535, "transfer");
        assert_eq!(write(65535).map(|b| b.len()), Ok(65535));
        assert_eq!(write(65536).unwrap_err()["error"], "TypeError");
    }
}
