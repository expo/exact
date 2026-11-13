//! @exact/serial on the Windows host (LLP 1133.037 D7–D9, D14, D21): Win32's communications functions. The ports
//! come from Configuration Manager (`CM_Get_Device_Interface_ListW` over the COM port interface class, each
//! interface's `PortName` read from the device's registry key); a port is opened with `CreateFileW` and no sharing
//! ("exclusive access", D5) for overlapped I/O, its line set with `SetCommState`, its signals with `EscapeCommFunction`
//! and read with `GetCommModemStatus`. A thread per connection reads it (`ReadFile` with a short `ReadIntervalTimeout`)
//! into the bytes not yet delivered, stopping while they reach the retained bound (D8). `ClearCommError` names a break,
//! framing, parity or overrun error at its offset (D9). A second thread writes the calls' bytes in call order and
//! answers each once the device has taken it, as Web Serial's writable stream does; `close` cancels the write in
//! flight and fails it and the ones queued behind it with `AbortError`. The I/O is overlapped because Windows runs one
//! call at a time on a handle opened without it, so a write the device does not take would stop the reads.
//!
//! WinRT's `SerialDevice` needs a package capability the unpackaged host cannot declare (D14), so this is Win32's,
//! declared here: the package names only `exact_painted` and `std`. Not built yet: the plug notifications; the list
//! is read again at each ask. `requestPort` (the `DevicePicker`) fails `NotSupportedError`.

use exact_painted::package::{json, Context, Json, Notifier, Package, Reply};
use std::collections::{BTreeMap, VecDeque};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

type Handle = *mut c_void;
const INVALID: Handle = -1isize as Handle;

#[repr(C)]
#[derive(Clone, Copy)]
struct Guid(u32, u16, u16, [u8; 8]);
/// The COM port interface class, `GUID_DEVINTERFACE_COMPORT` (ntddser.h).
const COMPORT: Guid = Guid(0x86E0_D1E0, 0x8089, 0x11D0, [0x9C, 0xE4, 0x08, 0x00, 0x3E, 0x30, 0x1F, 0x73]);

/// `DCB` (winbase.h): its bit fields are one `u32` here.
#[repr(C)]
#[derive(Default)]
struct Dcb {
    length: u32,
    baud: u32,
    flags: u32,
    reserved: u16,
    xon_lim: u16,
    xoff_lim: u16,
    byte_size: u8,
    parity: u8,
    stop_bits: u8,
    xon: i8,
    xoff: i8,
    error: i8,
    eof: i8,
    event: i8,
    reserved1: u16,
}
#[repr(C)]
struct Timeouts {
    read_interval: u32,
    read_multiplier: u32,
    read_constant: u32,
    write_multiplier: u32,
    write_constant: u32,
}
/// `OVERLAPPED` (minwinbase.h); a communications device ignores the offset.
#[repr(C)]
struct Overlapped {
    internal: usize,
    internal_high: usize,
    offset: u32,
    offset_high: u32,
    event: Handle,
}
impl Overlapped {
    fn new(event: Handle) -> Overlapped {
        Overlapped { internal: 0, internal_high: 0, offset: 0, offset_high: 0, event }
    }
}
#[repr(C)]
#[derive(Default)]
struct ComStat {
    flags: u32,
    in_queue: u32,
    out_queue: u32,
}

#[cfg_attr(windows, link(name = "kernel32"))]
extern "system" {
    fn CreateFileW(name: *const u16, access: u32, share: u32, security: *mut c_void, disposition: u32, flags: u32, template: Handle) -> Handle;
    fn ReadFile(file: Handle, buffer: *mut u8, len: u32, read: *mut u32, overlapped: *mut Overlapped) -> i32;
    fn WriteFile(file: Handle, buffer: *const u8, len: u32, written: *mut u32, overlapped: *mut Overlapped) -> i32;
    fn GetOverlappedResult(file: Handle, overlapped: *mut Overlapped, transferred: *mut u32, wait: i32) -> i32;
    fn CreateEventW(security: *mut c_void, manual_reset: i32, initial: i32, name: *const u16) -> Handle;
    fn CloseHandle(h: Handle) -> i32;
    fn CancelIoEx(h: Handle, overlapped: *mut c_void) -> i32;
    fn GetCommState(h: Handle, dcb: *mut Dcb) -> i32;
    fn SetCommState(h: Handle, dcb: *const Dcb) -> i32;
    fn SetCommTimeouts(h: Handle, t: *const Timeouts) -> i32;
    fn EscapeCommFunction(h: Handle, function: u32) -> i32;
    fn GetCommModemStatus(h: Handle, status: *mut u32) -> i32;
    fn ClearCommError(h: Handle, errors: *mut u32, stat: *mut ComStat) -> i32;
    fn GetLastError() -> u32;
}
#[cfg_attr(windows, link(name = "cfgmgr32"))]
extern "system" {
    fn CM_Get_Device_Interface_List_SizeW(len: *mut u32, guid: *const Guid, device: *const u16, flags: u32) -> u32;
    fn CM_Get_Device_Interface_ListW(guid: *const Guid, device: *const u16, buffer: *mut u16, len: u32, flags: u32) -> u32;
    fn CM_Get_Device_Interface_PropertyW(path: *const u16, key: *const PropertyKey, kind: *mut u32, buffer: *mut u8, len: *mut u32, flags: u32) -> u32;
    fn CM_Locate_DevNodeW(node: *mut u32, id: *const u16, flags: u32) -> u32;
    fn CM_Get_DevNode_PropertyW(node: u32, key: *const PropertyKey, kind: *mut u32, buffer: *mut u8, len: *mut u32, flags: u32) -> u32;
    fn CM_Open_DevNode_Key(node: u32, access: u32, profile: u32, disposition: u32, key: *mut Handle, flags: u32) -> u32;
}
#[cfg_attr(windows, link(name = "advapi32"))]
extern "system" {
    fn RegQueryValueExW(key: Handle, name: *const u16, reserved: *mut u32, kind: *mut u32, data: *mut u8, len: *mut u32) -> i32;
    fn RegCloseKey(key: Handle) -> i32;
}

#[repr(C)]
struct PropertyKey {
    guid: Guid,
    pid: u32,
}
/// `DEVPKEY_Device_InstanceId` and `DEVPKEY_Device_FriendlyName`, `DEVPKEY_Device_BusReportedDeviceDesc` (devpkey.h).
const INSTANCE_ID: PropertyKey = PropertyKey { guid: Guid(0x78C3_4FC8, 0x104A, 0x4ACA, [0x9E, 0xA4, 0x52, 0x4D, 0x52, 0x99, 0x6E, 0x57]), pid: 256 };
const BUS_DESCRIPTION: PropertyKey = PropertyKey { guid: Guid(0x540B_947E, 0x8B40, 0x45BC, [0xA8, 0xA2, 0x6A, 0x0B, 0x89, 0x4C, 0xBD, 0xA2]), pid: 4 };

const GENERIC_READ_WRITE: u32 = 0xC000_0000;
const OPEN_EXISTING: u32 = 3;
const FILE_FLAG_OVERLAPPED: u32 = 0x4000_0000;
const ERROR_IO_PENDING: u32 = 997;
const KEY_READ: u32 = 0x2_0019;
const SETRTS: u32 = 3;
const CLRRTS: u32 = 4;
const SETDTR: u32 = 5;
const CLRDTR: u32 = 6;
const SETBREAK: u32 = 8;
const CLRBREAK: u32 = 9;
const MS_CTS_ON: u32 = 0x10;
const MS_DSR_ON: u32 = 0x20;
const MS_RING_ON: u32 = 0x40;
const MS_RLSD_ON: u32 = 0x80;
const CE_RXPARITY: u32 = 0x4;
const CE_FRAME: u32 = 0x8;
const CE_BREAK: u32 = 0x10;
const CE_OVERRUN: u32 = 0x2;
const ERROR_ACCESS_DENIED: u32 = 5;
const MOST: usize = 16 * 1024 * 1024;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

#[derive(Clone)]
struct Port {
    id: String,
    name: String,
    path: String,
    vendor: Option<u64>,
    product: Option<u64>,
}

/// A string device property of an interface (`CM_Get_Device_Interface_PropertyW`) or its node.
fn property(path: &[u16], key: &PropertyKey, node: Option<u32>) -> Option<String> {
    let mut buf = [0u16; 512];
    let (mut kind, mut len) = (0u32, (buf.len() * 2) as u32);
    // SAFETY: a wide buffer of `len` bytes.
    let r = unsafe {
        match node {
            Some(n) => CM_Get_DevNode_PropertyW(n, key, &mut kind, buf.as_mut_ptr() as *mut u8, &mut len, 0),
            None => CM_Get_Device_Interface_PropertyW(path.as_ptr(), key, &mut kind, buf.as_mut_ptr() as *mut u8, &mut len, 0),
        }
    };
    (r == 0).then(|| from_wide(&buf))
}

fn ports() -> Vec<Port> {
    let mut len = 0u32;
    // SAFETY: the list's length, then the list, NUL-separated and double-NUL-terminated.
    let list = unsafe {
        if CM_Get_Device_Interface_List_SizeW(&mut len, &COMPORT, std::ptr::null(), 0) != 0 || len == 0 {
            return Vec::new();
        }
        let mut buffer = vec![0u16; len as usize];
        if CM_Get_Device_Interface_ListW(&COMPORT, std::ptr::null(), buffer.as_mut_ptr(), len, 0) != 0 {
            return Vec::new();
        }
        buffer
    };
    let mut out = Vec::new();
    for path in list.split(|c| *c == 0).filter(|p| !p.is_empty()) {
        let path: Vec<u16> = path.iter().copied().chain(std::iter::once(0)).collect();
        let Some(instance) = property(&path, &INSTANCE_ID, None) else {
            continue;
        };
        let mut node = 0u32;
        let id = wide(&instance);
        // SAFETY: a NUL-terminated instance id.
        if unsafe { CM_Locate_DevNodeW(&mut node, id.as_ptr(), 0) } != 0 {
            continue;
        }
        // The port's name (`COM3`) is the `PortName` value of the device's own registry key.
        let mut key: Handle = std::ptr::null_mut();
        let mut name = [0u16; 64];
        // SAFETY: the device key, read once and closed.
        unsafe {
            if CM_Open_DevNode_Key(node, KEY_READ, 0, 1, &mut key, 1) == 0 {
                let mut size = (name.len() * 2) as u32;
                RegQueryValueExW(key, wide("PortName").as_ptr(), std::ptr::null_mut(), std::ptr::null_mut(), name.as_mut_ptr() as *mut u8, &mut size);
                RegCloseKey(key);
            }
        }
        let com = from_wide(&name);
        if com.is_empty() {
            continue;
        }
        let hex = |tag: &str| instance.to_uppercase().split(tag).nth(1).and_then(|r| u64::from_str_radix(&r[..4.min(r.len())], 16).ok());
        let description = property(&path, &BUS_DESCRIPTION, Some(node));
        out.push(Port { id: com.clone(), name: description.unwrap_or_else(|| com.clone()), path: format!(r"\\.\{com}"), vendor: hex("VID_"), product: hex("PID_") });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

fn record(p: &Port, connected: bool) -> Json {
    json!({ "id": p.id, "name": p.name, "connected": connected, "granted": true, "usbVendorId": p.vendor, "usbProductId": p.product })
}

fn matches(p: &Port, filters: &Json) -> bool {
    let Some(filters) = filters.as_array().filter(|f| !f.is_empty()) else {
        return true;
    };
    filters.iter().any(|f| f["usbVendorId"].as_u64().is_none_or(|v| Some(v) == p.vendor) && f["usbProductId"].as_u64().is_none_or(|v| Some(v) == p.product))
}

#[derive(Default)]
struct Stream {
    pending: Vec<u8>,
    errors: Vec<Json>,
    position: usize,
    done: bool,
    ended: bool,
}

struct Shared(Handle);
// SAFETY: a Win32 handle may be used from any thread; the port's is closed after its reader and writer end.
unsafe impl Send for Shared {}
unsafe impl Sync for Shared {}

/// One `write` call: its bytes, how many the device has taken, and the call's answer.
struct Outgoing {
    bytes: Vec<u8>,
    sent: usize,
    reply: Reply,
}

/// What a connection's writer thread and the calls share: the writes not yet begun, in call order.
#[derive(Default)]
struct Outbox {
    writes: VecDeque<Outgoing>,
    closed: bool,
}

struct Connection {
    port: Port,
    handle: Arc<Shared>,
    options: Json,
    open: bool,
    stream: Arc<(Mutex<Stream>, Condvar)>,
    stop: Arc<AtomicBool>,
    outbox: Arc<(Mutex<Outbox>, Condvar)>,
    threads: Vec<std::thread::JoinHandle<()>>,
}

pub struct Serial {
    context: Context,
    connections: BTreeMap<String, Connection>,
    next: u64,
    remembered: Vec<String>,
}

/// Whether an overlapped read or write began: `started` is what `ReadFile` or `WriteFile` returned. Called at once,
/// before anything else can set the thread's last error.
fn begun(started: i32) -> std::io::Result<()> {
    // SAFETY: the calling thread's last error.
    if started != 0 || unsafe { GetLastError() } == ERROR_IO_PENDING { Ok(()) } else { Err(std::io::Error::last_os_error()) }
}

/// Waits for the read or write `ov` began: the bytes it moved, or why it failed (cancelled by `close`, or the port
/// gone).
fn completed(handle: Handle, ov: &mut Overlapped) -> std::io::Result<u32> {
    let mut n = 0u32;
    // SAFETY: `ov` began an operation on `handle` and is not touched again until this returns.
    if unsafe { GetOverlappedResult(handle, ov, &mut n, 1) } != 0 { Ok(n) } else { Err(std::io::Error::last_os_error()) }
}

fn read_loop(handle: Arc<Shared>, event: Shared, stream: Arc<(Mutex<Stream>, Condvar)>, stop: Arc<AtomicBool>, bound: usize, notifier: Notifier) {
    let mut buf = vec![0u8; 4096];
    let (lock, room) = &*stream;
    loop {
        let mut ov = Overlapped::new(event.0);
        let started = {
            let mut s = lock.lock().unwrap_or_else(|e| e.into_inner());
            while s.pending.len() >= bound && !stop.load(Ordering::Relaxed) {
                s = room.wait_timeout(s, std::time::Duration::from_millis(100)).unwrap_or_else(|e| e.into_inner()).0;
            }
            // `close` sets `stop` under this lock and then cancels, so a read begun here is one its cancel ends.
            if stop.load(Ordering::Relaxed) {
                break;
            }
            let take = bound.saturating_sub(s.pending.len()).clamp(1, buf.len());
            // SAFETY: `buf` and `ov` outlive the read, which `completed` waits for; the timeouts `open` set bound it.
            begun(unsafe { ReadFile(handle.0, buf.as_mut_ptr(), take as u32, std::ptr::null_mut(), &mut ov) })
        };
        let result = started.and_then(|()| completed(handle.0, &mut ov));
        let (ok, read) = (result.is_ok(), result.unwrap_or(0));
        let (mut errors, mut stat) = (0u32, ComStat::default());
        // SAFETY: the port's error state, read and cleared.
        unsafe { ClearCommError(handle.0, &mut errors, &mut stat) };
        let mut s = lock.lock().unwrap_or_else(|e| e.into_inner());
        let at = s.position + s.pending.len();
        for (bit, code) in [(CE_BREAK, "BreakError"), (CE_FRAME, "FramingError"), (CE_RXPARITY, "ParityError"), (CE_OVERRUN, "BufferOverrunError")] {
            if errors & bit != 0 {
                s.errors.push(json!({ "at": at, "code": code }));
            }
        }
        if !ok {
            if !stop.load(Ordering::Relaxed) {
                // ERROR_ACCESS_DENIED or ERROR_BAD_COMMAND: the port went away (D9).
                s.errors.push(json!({ "at": at, "code": "NetworkError" }));
                s.done = true;
            }
            drop(s);
            notifier.changed("serial.serialInput");
            break;
        }
        s.pending.extend_from_slice(&buf[..read as usize]);
        let changed = read > 0 || errors != 0;
        drop(s);
        if changed {
            notifier.changed("serial.serialInput");
        }
    }
    // SAFETY: this thread's event, closed once.
    unsafe { CloseHandle(event.0) };
}

/// The connection's writer: writes each write in call order and answers it when the device has taken every byte.
fn write_loop(handle: Arc<Shared>, event: Shared, outbox: Arc<(Mutex<Outbox>, Condvar)>) {
    let (lock, ready) = &*outbox;
    // The write in flight: its bytes stay here, out of the outbox `close` empties, until the write completes.
    let mut current: Option<Outgoing> = None;
    loop {
        let mut ov = Overlapped::new(event.0);
        let started = {
            let mut o = lock.lock().unwrap_or_else(|e| e.into_inner());
            while current.is_none() && o.writes.is_empty() && !o.closed {
                o = ready.wait(o).unwrap_or_else(|e| e.into_inner());
            }
            // `close` sets `closed` under this lock and then cancels, so a write begun here is one its cancel ends.
            if o.closed {
                break;
            }
            let w = current.get_or_insert_with(|| o.writes.pop_front().expect("a write waits"));
            let rest = &w.bytes[w.sent..];
            // SAFETY: the bytes and `ov` outlive the write, which `completed` waits for.
            begun(unsafe { WriteFile(handle.0, rest.as_ptr(), rest.len() as u32, std::ptr::null_mut(), &mut ov) })
        };
        match started.and_then(|()| completed(handle.0, &mut ov)) {
            Ok(n) => {
                let w = current.as_mut().expect("the write in flight");
                w.sent += n as usize;
                if w.sent == w.bytes.len() {
                    current.take().expect("the write in flight").reply.send(json!({}));
                }
            }
            Err(e) => {
                let mut o = lock.lock().unwrap_or_else(|e| e.into_inner());
                if o.closed {
                    break;
                }
                // The port went away (D9): the writes queued behind this one fail with it.
                let failed: Vec<Outgoing> = current.take().into_iter().chain(std::mem::take(&mut o.writes)).collect();
                drop(o);
                for w in failed {
                    w.reply.send(json!({ "error": "NetworkError", "message": e.to_string() }));
                }
            }
        }
    }
    if let Some(w) = current {
        w.reply.send(json!({ "error": "AbortError" }));
    }
    // SAFETY: this thread's event, closed once.
    unsafe { CloseHandle(event.0) };
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

impl Serial {
    fn open(&mut self, args: &Json) -> Json {
        let id = args["port"].as_str().unwrap_or_default();
        let Some(port) = ports().into_iter().find(|p| p.id == id) else {
            return json!({ "error": "NotFoundError" });
        };
        if self.connections.values().any(|c| c.open && c.port.id == port.id) {
            return json!({ "error": "InvalidStateError", "message": "already open" });
        }
        let baud = args["baudRate"].as_f64().unwrap_or(0.0);
        if !(baud > 0.0) {
            return json!({ "error": "TypeError", "message": "baudRate" });
        }
        let mut options = json!({ "dataBits": 8, "stopBits": 1, "parity": "none", "bufferSize": 255, "flowControl": "none" });
        for (k, v) in args.as_object().into_iter().flatten() {
            if k != "port" && !v.is_null() {
                options[k] = v.clone();
            }
        }
        let name = wide(&port.path);
        // SAFETY: no sharing: "exclusive access" (CreateFile, D5).
        let h = unsafe { CreateFileW(name.as_ptr(), GENERIC_READ_WRITE, 0, std::ptr::null_mut(), OPEN_EXISTING, FILE_FLAG_OVERLAPPED, std::ptr::null_mut()) };
        if h == INVALID {
            // SAFETY: the calling thread's last error.
            let code = unsafe { GetLastError() };
            return json!({ "error": if code == ERROR_ACCESS_DENIED { "InvalidStateError" } else { "NetworkError" }, "message": std::io::Error::last_os_error().to_string() });
        }
        let mut dcb = Dcb { length: std::mem::size_of::<Dcb>() as u32, ..Dcb::default() };
        // SAFETY: the port's state read, changed and set; timeouts that return what has arrived within 50 ms.
        let set = unsafe {
            GetCommState(h, &mut dcb) != 0 && {
                dcb.baud = baud as u32;
                // The DCB's bit fields (winbase.h): fBinary (bit 0); fParity (bit 1), so a parity error is reported;
                // fDtrControl = DTR_CONTROL_ENABLE (bits 4-5), DTR raised on open as the terminal's open raises it on
                // Linux and macOS and Chromium's Web Serial sets it on Windows; and fRtsControl (bits 12-13): enabled,
                // or RTS/CTS handshaking with fOutxCtsFlow (bit 2) for hardware flow control.
                dcb.flags = 1 | 1 << 1 | 1 << 4
                    | if options["flowControl"] == "hardware" { 1 << 2 | 2 << 12 } else { 1 << 12 };
                dcb.byte_size = options["dataBits"].as_u64().unwrap_or(8) as u8;
                dcb.parity = match options["parity"].as_str() { Some("odd") => 1, Some("even") => 2, _ => 0 };
                dcb.stop_bits = if options["stopBits"] == 2 { 2 } else { 0 };
                SetCommState(h, &dcb) != 0
            } && SetCommTimeouts(h, &Timeouts { read_interval: 50, read_multiplier: 0, read_constant: 50, write_multiplier: 0, write_constant: 0 }) != 0
        };
        // SAFETY: manual-reset events, one for each thread's overlapped calls, which that thread closes.
        let events = set.then(|| unsafe { [CreateEventW(std::ptr::null_mut(), 1, 0, std::ptr::null()), CreateEventW(std::ptr::null_mut(), 1, 0, std::ptr::null())] });
        let Some([reads, writes]) = events.filter(|e| e.iter().all(|e| !e.is_null())) else {
            let error = failed();
            // SAFETY: each closed once.
            unsafe {
                for e in events.into_iter().flatten().filter(|e| !e.is_null()) {
                    CloseHandle(e);
                }
                CloseHandle(h);
            }
            return error;
        };
        let bound = ((baud as usize) / 10 * 2).max(options["bufferSize"].as_u64().unwrap_or(255) as usize).min(MOST);
        self.next += 1;
        let connection = format!("serial-{}", self.next);
        let (handle, stream, stop) = (Arc::new(Shared(h)), Arc::new((Mutex::new(Stream::default()), Condvar::new())), Arc::new(AtomicBool::new(false)));
        let outbox = Arc::new((Mutex::new(Outbox::default()), Condvar::new()));
        let (hh, e, s, st, n) = (handle.clone(), Shared(reads), stream.clone(), stop.clone(), self.context.notifier());
        let reader = std::thread::spawn(move || read_loop(hh, e, s, st, bound, n));
        let (hh, e, o) = (handle.clone(), Shared(writes), outbox.clone());
        let writer = std::thread::spawn(move || write_loop(hh, e, o));
        if !self.remembered.contains(&port.id) {
            self.remembered.push(port.id.clone());
        }
        let answer = json!({ "connection": connection, "port": record(&port, true), "options": options });
        self.connections.insert(connection, Connection { port, handle, options, open: true, stream, stop, outbox, threads: vec![reader, writer] });
        self.context.changed("serial.getPorts");
        answer
    }

    fn connection(&self, args: &Json) -> Result<&Connection, Json> {
        match self.connections.get(args["connection"].as_str().unwrap_or_default()) {
            Some(c) if c.open => Ok(c),
            _ => Err(json!({ "error": "InvalidStateError" })),
        }
    }

    fn close(&mut self, connection: &str) -> Json {
        let Some(c) = self.connections.get_mut(connection).filter(|c| c.open) else {
            return json!({ "error": "InvalidStateError" });
        };
        c.open = false;
        // Set under the locks the threads begin their reads and writes under, so after these none begins.
        {
            let _s = c.stream.0.lock().unwrap_or_else(|e| e.into_inner());
            c.stop.store(true, Ordering::Relaxed);
        }
        c.stream.1.notify_all();
        let queued = {
            let mut o = c.outbox.0.lock().unwrap_or_else(|e| e.into_inner());
            o.closed = true;
            std::mem::take(&mut o.writes)
        };
        c.outbox.1.notify_all();
        // SAFETY: the read and write in flight are cancelled, so both threads end promptly.
        unsafe { CancelIoEx(c.handle.0, std::ptr::null_mut()) };
        for thread in c.threads.drain(..) {
            let _ = thread.join();
        }
        // SAFETY: closed once, after the threads that used it; closing it releases the port.
        unsafe { CloseHandle(c.handle.0) };
        // After the write that was in flight, which its thread failed as it ended.
        for w in queued {
            w.reply.send(json!({ "error": "AbortError" }));
        }
        self.context.changed("serial.serialConnection");
        self.context.changed("serial.serialInput");
        Json::Null
    }
}

impl Package for Serial {
    fn new(context: Context) -> Self {
        Serial { context, connections: BTreeMap::new(), next: 0, remembered: Vec::new() }
    }

    fn later(&mut self, name: &str, args: &Json, _preferred: Option<&Json>, reply: Reply) {
        if name == "write" {
            // Queued for the connection's writer, which answers when the device has taken the bytes.
            return match (self.connection(args), args["data"].as_str().and_then(unbase64)) {
                (Err(e), _) => reply.send(e),
                (_, None) => reply.send(json!({ "error": "TypeError" })),
                (Ok(c), Some(bytes)) => {
                    c.outbox.0.lock().unwrap_or_else(|e| e.into_inner()).writes.push_back(Outgoing { bytes, sent: 0, reply });
                    c.outbox.1.notify_one();
                }
            };
        }
        let answer = match name {
            "serialStatus" => json!({ "available": true, "attachedList": true, "state": "granted" }),
            "attachedPorts" => json!(ports().iter().filter(|p| matches(p, &args["filters"])).map(|p| record(p, true)).collect::<Vec<_>>()),
            "getPorts" => {
                let here = ports();
                json!(self.remembered.iter().map(|id| match here.iter().find(|p| &p.id == id) {
                    Some(p) => record(p, true),
                    None => json!({ "id": id, "name": id, "connected": false, "granted": true, "usbVendorId": null, "usbProductId": null }),
                }).collect::<Vec<_>>())
            }
            "requestPort" => json!({ "error": "NotSupportedError" }),
            "open" => self.open(args),
            "serialConnection" => match self.connections.get(args["connection"].as_str().unwrap_or_default()) {
                Some(c) => {
                    let gone = c.stream.0.lock().map(|s| s.done).unwrap_or(true);
                    json!({ "open": c.open && !gone, "connected": !gone, "options": c.options })
                }
                None => json!({ "open": false, "connected": false }),
            },
            "serialInput" => match self.connections.get(args["connection"].as_str().unwrap_or_default()) {
                None => json!({ "error": "InvalidStateError", "message": "no such connection" }),
                Some(c) => {
                    let (lock, room) = &*c.stream;
                    let mut s = lock.lock().unwrap_or_else(|e| e.into_inner());
                    if s.ended {
                        json!({ "error": "InvalidStateError", "message": "the stream has ended" })
                    } else {
                        let bytes = std::mem::take(&mut s.pending);
                        s.position += bytes.len();
                        let errors = std::mem::take(&mut s.errors);
                        let done = s.done || !c.open;
                        s.ended = done;
                        room.notify_all();
                        json!({ "data": base64(&bytes), "errors": errors, "done": done, "position": s.position })
                    }
                }
            },
            "setSignals" => match self.connection(args) {
                Err(e) => e,
                Ok(c) => {
                    let mut ok = true;
                    for (key, on, off) in [("dataTerminalReady", SETDTR, CLRDTR), ("requestToSend", SETRTS, CLRRTS), ("break", SETBREAK, CLRBREAK)] {
                        if let Some(v) = args[key].as_bool() {
                            // SAFETY: an escape function on the open port.
                            ok &= unsafe { EscapeCommFunction(c.handle.0, if v { on } else { off }) } != 0;
                        }
                    }
                    if ok { Json::Null } else { failed() }
                }
            },
            "getSignals" => match self.connection(args) {
                Err(e) => e,
                Ok(c) => {
                    let mut s = 0u32;
                    // SAFETY: the modem status read into `s`.
                    if unsafe { GetCommModemStatus(c.handle.0, &mut s) } == 0 {
                        failed()
                    } else {
                        json!({ "dataCarrierDetect": s & MS_RLSD_ON != 0, "clearToSend": s & MS_CTS_ON != 0, "ringIndicator": s & MS_RING_ON != 0, "dataSetReady": s & MS_DSR_ON != 0 })
                    }
                }
            },
            "close" => self.close(args["connection"].as_str().unwrap_or_default()),
            "forget" => {
                let id = args["port"].as_str().unwrap_or_default().to_owned();
                self.remembered.retain(|r| *r != id);
                let open: Vec<String> = self.connections.iter().filter(|(_, c)| c.port.id == id && c.open).map(|(h, _)| h.clone()).collect();
                for h in open {
                    self.close(&h);
                }
                self.context.changed("serial.getPorts");
                Json::Null
            }
            _ => json!({ "error": "NotSupportedError" }),
        };
        reply.send(if answer.is_null() { json!({}) } else { answer });
    }

    fn destroy(&mut self) {
        let open: Vec<String> = self.connections.iter().filter(|(_, c)| c.open).map(|(h, _)| h.clone()).collect();
        for h in open {
            self.close(&h);
        }
    }
}
