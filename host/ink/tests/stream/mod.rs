//! A recorded stream of `Device` calls, and its replay. @ref LLP 1172.023.006 D5.
//!
//! The crate's own tests do not cover compositing, and the pixel suites reach it only
//! through the adapter in `host/painted`. A stream records what the adapter asked of
//! the device for one scene, so the renderer can be checked and timed without it.
//!
//! A stream is the bytes `INKS`, a `u32` version and the recording device's name, then
//! records to the end of the file. A record is one mutating `Device` call, in the order
//! the adapter made it: a `u8` tag, a `u32` byte length, then its arguments. Integers are
//! little-endian `u32`s, bytes follow a `u32` count, and an instance is its 192 bytes. A
//! texture is named by the order of its creation, from 1; 0 is none. Queries are not
//! recorded. Retirements of textures (not targets) with no call between them are in
//! name order, so that two recordings of a scene are the same bytes.
//!
//! `host/painted/src/ink/stream_tests.rs` records the streams; `tests/replay.rs` replays them.

use exact_ink::device::{Device, Format, Result, TextureDesc, TextureUpload};
use exact_ink::instance::{Batch, Bindings, Frame, Scissor, TextureId};
use exact_ink::Instance;
use std::collections::HashMap;

const MAGIC: &[u8; 4] = b"INKS";
const VERSION: u32 = 1;

/// One device call. A `TextureId` here holds the stream's name for the texture.
pub enum Record {
    CreateTexture {
        id: TextureId,
        desc: TextureDesc,
    },
    CreateTarget {
        id: TextureId,
        size: [u32; 2],
    },
    /// `id` is `target` where the device kept its storage.
    ResizeTarget {
        id: TextureId,
        target: TextureId,
        size: [u32; 2],
    },
    Upload(TextureUpload),
    /// One `upload_textures` call.
    Uploads(Vec<TextureUpload>),
    GenerateMips(TextureId),
    Copy {
        source: TextureId,
        origin: [u32; 2],
        target: TextureId,
        dst: [u32; 2],
        size: [u32; 2],
    },
    Render {
        frame: Frame,
        target: TextureId,
        clear: Option<[f64; 4]>,
    },
    /// The SHA-256 of the bytes the recording device returned.
    Readback {
        texture: TextureId,
        sha256: [u8; 32],
    },
    Retire(TextureId),
    Trim,
}

pub struct Stream {
    /// The device that recorded it, as [`device_name`] gives it.
    pub device: String,
    pub records: Vec<Record>,
}

impl Stream {
    /// What the recording device read back, in order.
    pub fn readbacks(&self) -> impl Iterator<Item = [u8; 32]> + '_ {
        self.records.iter().filter_map(|record| match record {
            Record::Readback { sha256, .. } => Some(*sha256),
            _ => None,
        })
    }
}

/// The name a stream gives the device that recorded it.
pub fn device_name(device: &impl Device) -> String {
    format!("{} ({})", device.info().adapter, device.info().api)
}

/// `frame` with each texture it binds renamed.
pub fn renamed(frame: &Frame, name: impl Fn(TextureId) -> TextureId) -> Frame {
    let mut frame = frame.clone();
    for batch in &mut frame.batches {
        let b = &mut batch.bindings;
        for id in [
            &mut b.glyph,
            &mut b.color,
            &mut b.path,
            &mut b.ramp,
            &mut b.image,
            &mut b.mask,
            &mut b.destination,
        ] {
            *id = name(*id);
        }
    }
    frame
}

/// A replay in progress: the device's texture for each of the stream's live names.
pub struct Replay(pub HashMap<TextureId, TextureId>);

impl Default for Replay {
    fn default() -> Self {
        Self(HashMap::from([(TextureId(0), TextureId(0))]))
    }
}

impl Replay {
    /// Make one of the stream's calls on `device`. Returns a readback's bytes.
    pub fn step(&mut self, device: &mut impl Device, record: &Record) -> Result<Option<Vec<u8>>> {
        let at = |id: &TextureId| self.0[id];
        match record {
            Record::CreateTexture { id, desc } => {
                let texture = device.create_texture(*desc)?;
                self.0.insert(*id, texture);
            }
            Record::CreateTarget { id, size } => {
                let texture = device.create_target(size[0], size[1])?;
                self.0.insert(*id, texture);
            }
            Record::ResizeTarget { id, target, size } => {
                // A device that makes new storage retires the old.
                let old = self.0.remove(target).unwrap();
                let texture = device.resize_target(old, size[0], size[1])?;
                self.0.insert(*id, texture);
            }
            Record::Upload(u) => {
                device.upload_texture(at(&u.texture), u.origin, u.size, &u.data)?;
            }
            Record::Uploads(uploads) => {
                let uploads: Vec<_> = (uploads.iter())
                    .map(|u| TextureUpload {
                        texture: at(&u.texture),
                        origin: u.origin,
                        size: u.size,
                        data: u.data.clone(),
                    })
                    .collect();
                device.upload_textures(&uploads)?;
            }
            Record::GenerateMips(texture) => {
                device.generate_mips(at(texture))?;
            }
            Record::Copy {
                source,
                origin,
                target,
                dst,
                size,
            } => {
                device.copy_region(at(source), *origin, at(target), *dst, *size)?;
            }
            Record::Render {
                frame,
                target,
                clear,
            } => {
                device.render(&renamed(frame, |id| at(&id)), at(target), *clear)?;
            }
            Record::Readback { texture, .. } => return device.readback(at(texture)).map(Some),
            Record::Retire(texture) => device.retire_texture(self.0.remove(texture).unwrap())?,
            Record::Trim => device.trim()?,
        }
        Ok(None)
    }
}

/// Make the stream's calls on `device`. Returns the bytes of each readback, in order.
pub fn replay(device: &mut impl Device, records: &[Record]) -> Result<Vec<Vec<u8>>> {
    let mut replay = Replay::default();
    let mut readbacks = Vec::new();
    for record in records {
        readbacks.extend(replay.step(device, record)?);
    }
    Ok(readbacks)
}

fn put(out: &mut Vec<u8>, words: &[u32]) {
    for word in words {
        out.extend(word.to_le_bytes());
    }
}

fn put_upload(out: &mut Vec<u8>, u: &TextureUpload) {
    let [x, y, w, h] = [u.origin[0], u.origin[1], u.size[0], u.size[1]];
    put(out, &[u.texture.0 as u32, x, y, w, h, u.data.len() as u32]);
    out.extend(&u.data);
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn bytes(&mut self, count: usize) -> &'a [u8] {
        let (bytes, rest) = self.0.split_at_checked(count).expect("a truncated stream");
        self.0 = rest;
        bytes
    }
    fn u32(&mut self) -> u32 {
        u32::from_le_bytes(self.bytes(4).try_into().unwrap())
    }
    fn pair(&mut self) -> [u32; 2] {
        [self.u32(), self.u32()]
    }
    fn id(&mut self) -> TextureId {
        TextureId(self.u32().into())
    }
    fn counted(&mut self) -> &'a [u8] {
        let count = self.u32() as usize;
        self.bytes(count)
    }
    fn upload(&mut self) -> TextureUpload {
        TextureUpload {
            texture: self.id(),
            origin: self.pair(),
            size: self.pair(),
            data: self.counted().to_vec(),
        }
    }
}

impl Stream {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        put(&mut out, &[VERSION, self.device.len() as u32]);
        out.extend(self.device.as_bytes());
        for record in &self.records {
            let mut b = Vec::new();
            let tag = match record {
                Record::CreateTexture { id, desc: d } => {
                    put(&mut b, &[id.0 as u32, d.width, d.height]);
                    put(&mut b, &[d.format.channels(), d.mipmapped.into()]);
                    1
                }
                Record::CreateTarget { id, size } => {
                    put(&mut b, &[id.0 as u32, size[0], size[1]]);
                    2
                }
                Record::ResizeTarget { id, target, size } => {
                    put(&mut b, &[id.0 as u32, target.0 as u32, size[0], size[1]]);
                    3
                }
                Record::Upload(u) => {
                    put_upload(&mut b, u);
                    4
                }
                Record::Uploads(uploads) => {
                    put(&mut b, &[uploads.len() as u32]);
                    uploads.iter().for_each(|u| put_upload(&mut b, u));
                    5
                }
                Record::GenerateMips(texture) => {
                    put(&mut b, &[texture.0 as u32]);
                    6
                }
                Record::Copy {
                    source,
                    origin,
                    target,
                    dst,
                    size,
                } => {
                    put(&mut b, &[source.0 as u32, origin[0], origin[1]]);
                    put(&mut b, &[target.0 as u32, dst[0], dst[1], size[0], size[1]]);
                    7
                }
                Record::Render {
                    frame,
                    target,
                    clear,
                } => {
                    put(&mut b, &[target.0 as u32, clear.is_some().into()]);
                    for channel in clear.iter().flatten() {
                        b.extend(channel.to_le_bytes());
                    }
                    put(&mut b, &[frame.instances.len() as u32]);
                    b.extend(bytemuck::cast_slice::<Instance, u8>(&frame.instances));
                    put(&mut b, &[frame.batches.len() as u32]);
                    for batch in &frame.batches {
                        let (s, t) = (batch.scissor, batch.bindings);
                        put(&mut b, &[batch.replace.into()]);
                        put(&mut b, &[batch.instances.start, batch.instances.end]);
                        put(&mut b, &[s.x, s.y, s.width, s.height, t.repeat.into()]);
                        let ids = [t.glyph, t.color, t.path, t.ramp, t.image, t.mask];
                        put(&mut b, &ids.map(|id| id.0 as u32));
                        put(&mut b, &[t.destination.0 as u32]);
                    }
                    8
                }
                Record::Readback { texture, sha256 } => {
                    put(&mut b, &[texture.0 as u32]);
                    b.extend(sha256);
                    9
                }
                Record::Retire(texture) => {
                    put(&mut b, &[texture.0 as u32]);
                    10
                }
                Record::Trim => 11,
            };
            out.push(tag);
            put(&mut out, &[b.len() as u32]);
            out.extend(b);
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Self {
        let mut r = Reader(bytes);
        assert_eq!(r.bytes(4), MAGIC, "not a device stream");
        assert_eq!(r.u32(), VERSION, "the stream's version");
        let device = String::from_utf8(r.counted().to_vec()).expect("the device's name");
        let mut records = Vec::new();
        while !r.0.is_empty() {
            let tag = r.bytes(1)[0];
            let mut r = Reader(r.counted());
            records.push(match tag {
                1 => Record::CreateTexture {
                    id: r.id(),
                    desc: TextureDesc {
                        width: r.u32(),
                        height: r.u32(),
                        format: match r.u32() {
                            1 => Format::Alpha,
                            _ => Format::Rgba,
                        },
                        mipmapped: r.u32() != 0,
                    },
                },
                2 => Record::CreateTarget {
                    id: r.id(),
                    size: r.pair(),
                },
                3 => Record::ResizeTarget {
                    id: r.id(),
                    target: r.id(),
                    size: r.pair(),
                },
                4 => Record::Upload(r.upload()),
                5 => Record::Uploads((0..r.u32()).map(|_| r.upload()).collect()),
                6 => Record::GenerateMips(r.id()),
                7 => Record::Copy {
                    source: r.id(),
                    origin: r.pair(),
                    target: r.id(),
                    dst: r.pair(),
                    size: r.pair(),
                },
                8 => {
                    let target = r.id();
                    let clear = (r.u32() != 0).then(|| {
                        [(); 4].map(|_| f64::from_le_bytes(r.bytes(8).try_into().unwrap()))
                    });
                    let instances = r.u32() as usize * size_of::<Instance>();
                    let instances = r.bytes(instances).as_chunks::<{ size_of::<Instance>() }>();
                    let instances = (instances.0.iter())
                        .map(|bytes| bytemuck::pod_read_unaligned(bytes))
                        .collect();
                    let batches = (0..r.u32())
                        .map(|_| Batch {
                            replace: r.u32() != 0,
                            instances: r.u32()..r.u32(),
                            scissor: Scissor {
                                x: r.u32(),
                                y: r.u32(),
                                width: r.u32(),
                                height: r.u32(),
                            },
                            bindings: Bindings {
                                repeat: r.u32() != 0,
                                glyph: r.id(),
                                color: r.id(),
                                path: r.id(),
                                ramp: r.id(),
                                image: r.id(),
                                mask: r.id(),
                                destination: r.id(),
                            },
                        })
                        .collect();
                    Record::Render {
                        frame: Frame { instances, batches },
                        target,
                        clear,
                    }
                }
                9 => Record::Readback {
                    texture: r.id(),
                    sha256: r.bytes(32).try_into().unwrap(),
                },
                10 => Record::Retire(r.id()),
                11 => Record::Trim,
                tag => panic!("unknown record {tag}"),
            });
        }
        Self { device, records }
    }
}
