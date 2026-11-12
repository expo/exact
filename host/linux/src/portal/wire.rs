//! The D-Bus wire format, as much as a portal call needs: little-endian
//! messages whose bodies hold any complete type, read and written by
//! signature (the [D-Bus Specification](https://dbus.freedesktop.org/doc/dbus-specification.html),
//! "Type System" and "Message Protocol"). Unix file descriptors are never
//! sent or taken.

/// A value of any D-Bus type. Integers other than `y` and `u` are read as
/// [`Value::Num`] and never written.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value {
    Byte(u8),
    Bool(bool),
    U32(u32),
    Num(i64),
    F64(f64),
    Str(String),
    Path(String),
    Sig(String),
    /// The element signature, then the elements.
    Array(String, Vec<Value>),
    Struct(Vec<Value>),
    /// A dictionary entry, `{kv}`.
    Entry(Box<Value>, Box<Value>),
    Variant(Box<Value>),
}

impl Value {
    pub(crate) fn str(s: &str) -> Value {
        Value::Str(s.to_owned())
    }

    /// An `a{sv}` from its entries.
    pub(crate) fn dict(entries: Vec<(&str, Value)>) -> Value {
        let entries = entries
            .into_iter()
            .map(|(k, v)| {
                Value::Entry(
                    Box::new(Value::str(k)),
                    Box::new(Value::Variant(Box::new(v))),
                )
            })
            .collect();
        Value::Array("{sv}".into(), entries)
    }

    /// The value under `key` in an `a{sv}`, unwrapped from its variant.
    pub(crate) fn get(&self, key: &str) -> Option<&Value> {
        let Value::Array(_, entries) = self else {
            return None;
        };
        entries.iter().find_map(|e| match e {
            Value::Entry(k, v) if **k == Value::Str(key.into()) => match &**v {
                Value::Variant(inner) => Some(&**inner),
                other => Some(other),
            },
            _ => None,
        })
    }

    pub(crate) fn signature(&self) -> String {
        match self {
            Value::Byte(_) => "y".into(),
            Value::Bool(_) => "b".into(),
            Value::U32(_) => "u".into(),
            Value::Num(_) => "x".into(),
            Value::F64(_) => "d".into(),
            Value::Str(_) => "s".into(),
            Value::Path(_) => "o".into(),
            Value::Sig(_) => "g".into(),
            Value::Array(element, _) => format!("a{element}"),
            Value::Struct(fields) => format!(
                "({})",
                fields.iter().map(Value::signature).collect::<String>()
            ),
            Value::Entry(k, v) => format!("{{{}{}}}", k.signature(), v.signature()),
            Value::Variant(_) => "v".into(),
        }
    }
}

/// A type code's alignment.
fn alignment(code: u8) -> usize {
    match code {
        b'n' | b'q' => 2,
        b'b' | b'i' | b'u' | b'h' | b's' | b'o' | b'a' => 4,
        b'x' | b't' | b'd' | b'(' | b'{' => 8,
        _ => 1,
    }
}

fn pad(buf: &mut Vec<u8>, to: usize) {
    while !buf.len().is_multiple_of(to) {
        buf.push(0);
    }
}

fn put_str(buf: &mut Vec<u8>, s: &str) {
    pad(buf, 4);
    buf.extend_from_slice(&(s.len() as u32).to_le_bytes());
    buf.extend_from_slice(s.as_bytes());
    buf.push(0);
}

/// Write `v` at the end of `buf`, aligned as it would be in a message
/// that begins at `buf`'s start.
pub(crate) fn write(buf: &mut Vec<u8>, v: &Value) {
    match v {
        Value::Byte(b) => buf.push(*b),
        Value::Bool(b) => {
            pad(buf, 4);
            buf.extend_from_slice(&u32::from(*b).to_le_bytes());
        }
        Value::U32(n) => {
            pad(buf, 4);
            buf.extend_from_slice(&n.to_le_bytes());
        }
        Value::Num(n) => {
            pad(buf, 8);
            buf.extend_from_slice(&n.to_le_bytes());
        }
        Value::F64(f) => {
            pad(buf, 8);
            buf.extend_from_slice(&f.to_le_bytes());
        }
        Value::Str(s) | Value::Path(s) => put_str(buf, s),
        Value::Sig(s) => {
            buf.push(s.len() as u8);
            buf.extend_from_slice(s.as_bytes());
            buf.push(0);
        }
        Value::Array(element, items) => {
            pad(buf, 4);
            let at = buf.len();
            buf.extend_from_slice(&[0; 4]);
            // The length counts from the first element, after its padding.
            pad(
                buf,
                alignment(element.as_bytes().first().copied().unwrap_or(b'y')),
            );
            let start = buf.len();
            for item in items {
                write(buf, item);
            }
            let len = (buf.len() - start) as u32;
            buf[at..at + 4].copy_from_slice(&len.to_le_bytes());
        }
        Value::Struct(fields) => {
            pad(buf, 8);
            for field in fields {
                write(buf, field);
            }
        }
        Value::Entry(k, v) => {
            pad(buf, 8);
            write(buf, k);
            write(buf, v);
        }
        Value::Variant(inner) => {
            write(buf, &Value::Sig(inner.signature()));
            write(buf, inner);
        }
    }
}

/// The first complete type in `sig`, and the rest.
fn split(sig: &str) -> Option<(&str, &str)> {
    let bytes = sig.as_bytes();
    let mut depth = 0usize;
    for (i, &c) in bytes.iter().enumerate() {
        match c {
            b'a' => continue,
            b'(' | b'{' => depth += 1,
            b')' | b'}' => depth = depth.checked_sub(1)?,
            _ => {}
        }
        if depth == 0 {
            return Some(sig.split_at(i + 1));
        }
    }
    None
}

/// The specification's limit on nesting (32 arrays and 32 structures).
const DEPTH: usize = 64;

struct Reader<'a> {
    buf: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn align(&mut self, to: usize) {
        self.at = self.at.div_ceil(to) * to;
    }
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let bytes = self.buf.get(self.at..self.at.checked_add(n)?)?;
        self.at += n;
        Some(bytes)
    }
    fn fixed<const N: usize>(&mut self) -> Option<[u8; N]> {
        self.align(N);
        self.take(N)?.try_into().ok()
    }
    fn u32(&mut self) -> Option<u32> {
        self.fixed::<4>().map(u32::from_le_bytes)
    }
    fn text(&mut self, len: usize) -> Option<String> {
        let s = std::str::from_utf8(self.take(len)?).ok()?.to_owned();
        (self.take(1)? == [0]).then_some(s)
    }

    /// One value of the complete type `sig`.
    fn value(&mut self, sig: &str, depth: usize) -> Option<Value> {
        if depth > DEPTH {
            return None;
        }
        let code = *sig.as_bytes().first()?;
        Some(match code {
            b'y' => Value::Byte(self.take(1)?[0]),
            b'b' => Value::Bool(self.u32()? != 0),
            b'u' => Value::U32(self.u32()?),
            b'n' => Value::Num(i16::from_le_bytes(self.fixed()?).into()),
            b'q' => Value::Num(u16::from_le_bytes(self.fixed()?).into()),
            b'i' | b'h' => Value::Num((self.u32()? as i32).into()),
            b'x' => Value::Num(i64::from_le_bytes(self.fixed()?)),
            b't' => Value::Num(u64::from_le_bytes(self.fixed()?) as i64),
            b'd' => Value::F64(f64::from_le_bytes(self.fixed()?)),
            b's' | b'o' => {
                let len = self.u32()? as usize;
                let s = self.text(len)?;
                if code == b's' {
                    Value::Str(s)
                } else {
                    Value::Path(s)
                }
            }
            b'g' => {
                let len = self.take(1)?[0] as usize;
                Value::Sig(self.text(len)?)
            }
            b'v' => {
                let len = self.take(1)?[0] as usize;
                let inner = self.text(len)?;
                let (one, rest) = split(&inner)?;
                if !rest.is_empty() {
                    return None;
                }
                Value::Variant(Box::new(self.value(one, depth + 1)?))
            }
            b'a' => {
                let element = &sig[1..];
                let len = self.u32()? as usize;
                self.align(alignment(*element.as_bytes().first()?));
                let end = self.at.checked_add(len).filter(|&e| e <= self.buf.len())?;
                let mut items = Vec::new();
                while self.at < end {
                    items.push(self.value(element, depth + 1)?);
                }
                (self.at == end).then_some(())?;
                Value::Array(element.to_owned(), items)
            }
            b'(' | b'{' => {
                self.align(8);
                let mut inner = &sig[1..sig.len() - 1];
                let mut fields = Vec::new();
                while !inner.is_empty() {
                    let (one, rest) = split(inner)?;
                    fields.push(self.value(one, depth + 1)?);
                    inner = rest;
                }
                if code == b'{' {
                    let [k, v]: [Value; 2] = fields.try_into().ok()?;
                    Value::Entry(Box::new(k), Box::new(v))
                } else {
                    Value::Struct(fields)
                }
            }
            _ => return None,
        })
    }

    /// The values of a run of complete types.
    fn values(&mut self, mut sig: &str) -> Option<Vec<Value>> {
        let mut out = Vec::new();
        while !sig.is_empty() {
            let (one, rest) = split(sig)?;
            out.push(self.value(one, 0)?);
            sig = rest;
        }
        Some(out)
    }
}

pub(crate) const METHOD_CALL: u8 = 1;
pub(crate) const METHOD_RETURN: u8 = 2;
pub(crate) const ERROR: u8 = 3;
pub(crate) const SIGNAL: u8 = 4;

/// The largest message this reader takes. The specification allows 128 MiB;
/// a portal's answer is a list of URIs.
const LARGEST: usize = 16 << 20;

/// A message read from the bus.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Message {
    pub kind: u8,
    pub serial: u32,
    pub reply_serial: Option<u32>,
    pub path: String,
    pub interface: String,
    pub member: String,
    pub error_name: String,
    pub destination: String,
    pub sender: String,
    pub signature: String,
    pub body: Vec<Value>,
}

/// A message of `kind` to `destination` (empty: none), with `body`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn message(
    kind: u8,
    serial: u32,
    reply_serial: Option<u32>,
    destination: &str,
    path: &str,
    interface: &str,
    member: &str,
    body: &[Value],
) -> Vec<u8> {
    let mut encoded = Vec::new();
    for v in body {
        write(&mut encoded, v);
    }
    let field =
        |code: u8, v: Value| Value::Struct(vec![Value::Byte(code), Value::Variant(Box::new(v))]);
    let mut fields = Vec::new();
    if !path.is_empty() {
        fields.push(field(1, Value::Path(path.into())));
    }
    if !interface.is_empty() {
        fields.push(field(2, Value::str(interface)));
    }
    if !member.is_empty() {
        fields.push(field(3, Value::str(member)));
    }
    if let Some(n) = reply_serial {
        fields.push(field(5, Value::U32(n)));
    }
    if !destination.is_empty() {
        fields.push(field(6, Value::str(destination)));
    }
    if !body.is_empty() {
        fields.push(field(
            8,
            Value::Sig(body.iter().map(Value::signature).collect()),
        ));
    }
    let mut m = Vec::new();
    for v in [
        Value::Byte(b'l'),
        Value::Byte(kind),
        Value::Byte(0),
        Value::Byte(1),
        Value::U32(encoded.len() as u32),
        Value::U32(serial),
        Value::Array("(yv)".into(), fields),
    ] {
        write(&mut m, &v);
    }
    pad(&mut m, 8);
    m.extend_from_slice(&encoded);
    m
}

/// The length of the message whose first 16 bytes are `head`, or None for
/// one this reader does not take (big-endian, or too large).
pub(crate) fn message_len(head: &[u8; 16]) -> Option<usize> {
    if head[0] != b'l' {
        return None;
    }
    let body = u32::from_le_bytes(head[4..8].try_into().ok()?) as usize;
    let fields = u32::from_le_bytes(head[12..16].try_into().ok()?) as usize;
    let len = (16 + fields).div_ceil(8) * 8 + body;
    (len <= LARGEST).then_some(len)
}

/// The whole message in `buf`, as [`message_len`] measured it.
pub(crate) fn parse(buf: &[u8]) -> Option<Message> {
    let head: &[u8; 16] = buf.get(..16)?.try_into().ok()?;
    let total = message_len(head)?;
    let mut r = Reader {
        buf: buf.get(..total)?,
        at: 0,
    };
    let header = r.values("yyyyuua(yv)")?;
    let mut m = Message::default();
    if let (Value::Byte(kind), Value::U32(serial)) = (&header[1], &header[5]) {
        (m.kind, m.serial) = (*kind, *serial);
    }
    let Value::Array(_, fields) = &header[6] else {
        return None;
    };
    for f in fields {
        let Value::Struct(f) = f else { continue };
        let (Value::Byte(code), Value::Variant(v)) = (&f[0], &f[1]) else {
            continue;
        };
        match (code, &**v) {
            (1, Value::Path(s)) => m.path = s.clone(),
            (2, Value::Str(s)) => m.interface = s.clone(),
            (3, Value::Str(s)) => m.member = s.clone(),
            (4, Value::Str(s)) => m.error_name = s.clone(),
            (5, Value::U32(n)) => m.reply_serial = Some(*n),
            (6, Value::Str(s)) => m.destination = s.clone(),
            (7, Value::Str(s)) => m.sender = s.clone(),
            (8, Value::Sig(s)) => m.signature = s.clone(),
            _ => {}
        }
    }
    r.align(8);
    let body = &buf[r.at..total];
    m.body = Reader { buf: body, at: 0 }.values(&m.signature)?;
    Some(m)
}
