//! Baked sampled sounds. The bake decodes WAV and Ogg Vorbis; a module only
//! undoes this layout, so no audio decoder is linked into a game.
//! @ref LLP 1046.003 §AU4 (where decoding happens)
use crate::Data;
use std::{collections::BTreeMap, sync::Arc};

/// Resident 16-bit PCM for every delivered sound in one world, counted once per asset.
/// A delivery that would exceed it is refused by name before setup can run.
pub const SOUND_BYTE_BUDGET: usize = 32 * 1024 * 1024;
/// Source rates accepted by the bake and at delivery; outputs resample to the device.
pub const SOUND_RATES: std::ops::RangeInclusive<u32> = 8_000..=192_000;

/// Startup metadata for the sounds in `Game::STREAMED` (LLP 1046.003 AU4).
pub const SOUND_CATALOG: &str = "exact-streamed-sounds.bin";

/// A sample's simulation-visible format and the identity of the PCM the mixer may use.
#[derive(Data, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SoundInfo {
    /// Frames per second.
    pub rate: u32,
    /// One or two interleaved channels.
    pub channels: u32,
    /// Frames per channel.
    pub frames: u32,
    /// Identity of the baked sound record.
    pub digest: u64,
}

/// The `.sound` record: 16-bit PCM, channels planar, each channel stored as wrapping
/// first differences, which gzip compresses 1.3–2.2× smaller than interleaved PCM.
/// @ref LLP 1046.003 §AU4 (where decoding happens)
#[derive(Data, Default, Clone, Debug, PartialEq)]
pub struct SoundData {
    /// Frames per second.
    pub rate: u32,
    /// One (mono) or two (left, right).
    pub channels: u32,
    /// Frames per channel.
    pub frames: u32,
    /// `channels * frames` differences as `i16` bit patterns, one channel after another.
    pub deltas: Vec<u16>,
}
impl SoundData {
    /// Describe this record for the startup catalog without allocating decoded PCM.
    pub fn info(&self) -> SoundInfo {
        SoundInfo {
            rate: self.rate,
            channels: self.channels,
            frames: self.frames,
            digest: crate::hash::of(self),
        }
    }

    /// Encode interleaved 16-bit samples.
    pub fn encode(rate: u32, channels: u32, interleaved: &[i16]) -> Result<Self, String> {
        if !(1..=2).contains(&channels) {
            return Err(format!("{channels} channels; expected mono or stereo"));
        }
        let c = channels as usize;
        if !interleaved.len().is_multiple_of(c) {
            return Err("samples end mid-frame".into());
        }
        let frames = u32::try_from(interleaved.len() / c).map_err(|_| "too many frames")?;
        let mut deltas = Vec::with_capacity(interleaved.len());
        for channel in 0..c {
            let mut previous = 0i16;
            for frame in interleaved.chunks_exact(c) {
                deltas.push(frame[channel].wrapping_sub(previous) as u16);
                previous = frame[channel];
            }
        }
        let data = Self {
            rate,
            channels,
            frames,
            deltas,
        };
        data.validate()?;
        Ok(data)
    }
    /// Resident bytes once decoded.
    pub fn pcm_bytes(&self) -> usize {
        (self.frames as usize)
            .saturating_mul(self.channels as usize)
            .saturating_mul(2)
    }
    /// Refuse malformed or oversized records before allocating PCM.
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=2).contains(&self.channels) {
            return Err(format!(
                "{} channels; expected mono or stereo",
                self.channels
            ));
        }
        if !SOUND_RATES.contains(&self.rate) {
            return Err(format!(
                "{} Hz is outside {}..={} Hz",
                self.rate,
                SOUND_RATES.start(),
                SOUND_RATES.end()
            ));
        }
        if self.frames == 0 {
            return Err("sound has no frames".into());
        }
        if self.pcm_bytes() > SOUND_BYTE_BUDGET {
            return Err(format!(
                "{} of 16-bit PCM exceeds the {} sound residency budget",
                mib(self.pcm_bytes()),
                mib(SOUND_BYTE_BUDGET)
            ));
        }
        if self.deltas.len() != self.frames as usize * self.channels as usize {
            return Err("sample count does not match frames × channels".into());
        }
        Ok(())
    }
    /// Interleaved 16-bit PCM.
    pub fn samples(&self) -> Vec<i16> {
        let (c, frames) = (self.channels as usize, self.frames as usize);
        let mut out = vec![0i16; c * frames];
        for (channel, deltas) in self.deltas.chunks_exact(frames).enumerate() {
            let mut value = 0i16;
            for (frame, delta) in deltas.iter().enumerate() {
                value = value.wrapping_add(*delta as i16);
                out[frame * c + channel] = value;
            }
        }
        out
    }
}
pub(crate) fn mib(bytes: usize) -> String {
    format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
}

/// A delivered sound, resident once in its world's asset store. Presentation
/// executors share this allocation; simulation reads only registered metadata.
#[derive(Clone, Debug)]
pub struct SoundAsset {
    /// Frames per second.
    pub rate: u32,
    /// One or two.
    pub channels: u32,
    /// Frames per channel.
    pub frames: u32,
    /// Interleaved 16-bit PCM.
    pub samples: Arc<[i16]>,
    /// Content identity of the delivered record.
    pub digest: u64,
}
impl SoundAsset {
    fn new(data: &SoundData, digest: u64) -> Self {
        Self {
            rate: data.rate,
            channels: data.channels,
            frames: data.frames,
            samples: data.samples().into(),
            digest,
        }
    }
    /// Resident bytes.
    pub fn bytes(&self) -> usize {
        self.samples.len() * 2
    }
}

impl<G: crate::Game> crate::Sim<G> {
    pub(crate) fn deliver_sound_catalog(
        &mut self,
        name: &str,
        catalog: BTreeMap<String, SoundInfo>,
    ) -> Result<(), String> {
        let expected: std::collections::BTreeSet<_> = G::STREAMED
            .iter()
            .copied()
            .filter(|n| n.ends_with(".sound"))
            .collect();
        if name != SOUND_CATALOG
            || expected.is_empty()
            || !catalog.keys().map(String::as_str).eq(expected)
        {
            return Err(
                "the sound catalog must describe exactly the sounds in Game::STREAMED".into(),
            );
        }
        let mut bytes: u64 = self
            .world()
            .assets
            .sounds
            .iter()
            .filter(|(name, _)| !catalog.contains_key(name.as_str()))
            .map(|(_, sound)| sound.bytes() as u64)
            .sum();
        for (sample, info) in &catalog {
            if !SOUND_RATES.contains(&info.rate)
                || !(1..=2).contains(&info.channels)
                || info.frames == 0
            {
                return Err(format!("streamed sound `{sample}` has an invalid format"));
            }
            bytes += u64::from(info.channels) * u64::from(info.frames) * 2;
            if bytes > SOUND_BYTE_BUDGET as u64 {
                return Err(format!(
                    "sound `{sample}` exceeds the combined {} sound residency budget",
                    mib(SOUND_BYTE_BUDGET)
                ));
            }
        }
        let digest = crate::hash::of(&catalog);
        let assets = &mut self.world_mut().assets;
        if assets.identity(name).is_some_and(|old| old != digest) {
            return Err("the sound catalog changed; restart the game".into());
        }
        assets.identify(name, digest);
        assets.sound_catalog = Some(Arc::new(catalog));
        assets.states.insert(name.into(), super::AssetState::Loaded);
        Ok(())
    }

    pub(crate) fn deliver_sound(&mut self, name: &str, data: SoundData) -> Result<(), String> {
        let expected = self
            .world()
            .assets
            .sound_catalog
            .as_deref()
            .and_then(|c| c.get(name))
            .copied();
        if !G::ASSETS.contains(&name) && expected.is_none() {
            return Err(format!("sound `{name}` is not declared by Game::ASSETS"));
        }
        data.validate()?;
        let info = data.info();
        if expected.is_some_and(|expected| expected != info) {
            return Err(format!(
                "streamed sound `{name}` differs from its baked catalog"
            ));
        }
        let assets = &mut self.world_mut().assets;
        if assets.identity(name).is_some_and(|old| old != info.digest) {
            return Err(format!(
                "sound `{name}` cannot change after delivery; restart with the new sound"
            ));
        }
        if expected.is_none() {
            // Reserve the streamed catalog even when required sounds arrive after it.
            let catalog = assets.sound_catalog.as_deref();
            let resident: usize = assets
                .sounds
                .iter()
                .filter(|(other, _)| {
                    *other != name && !catalog.is_some_and(|c| c.contains_key(other.as_str()))
                })
                .map(|(_, sound)| sound.bytes())
                .sum();
            let reserved: usize = catalog
                .into_iter()
                .flat_map(|c| c.values())
                .map(|i| i.frames as usize * i.channels as usize * 2)
                .sum();
            let used = resident + reserved;
            if data.pcm_bytes() > SOUND_BYTE_BUDGET - used {
                return Err(format!(
                    "{} of 16-bit PCM with {} already {} exceeds the {} sound residency budget",
                    mib(data.pcm_bytes()),
                    mib(used),
                    if reserved == 0 {
                        "resident"
                    } else {
                        "reserved"
                    },
                    mib(SOUND_BYTE_BUDGET)
                ));
            }
            assets.identify(name, info.digest);
        }
        assets
            .sounds
            .insert(name.into(), Arc::new(SoundAsset::new(&data, info.digest)));
        assets.states.insert(name.into(), super::AssetState::Loaded);
        Ok(())
    }
    /// Resident PCM for the mixer, including streamed sounds. Game callbacks receive only a World.
    pub fn presentation_sound(&self, name: &str) -> Option<&SoundAsset> {
        self.world().assets.sounds.get(name).map(Arc::as_ref)
    }
}

impl crate::World {
    /// A declared, delivered sound's PCM, for presentation executors.
    pub fn sound_asset(&self, name: &str) -> Option<&SoundAsset> {
        self.assets
            .declared
            .contains(name)
            .then(|| self.assets.sounds.get(name).map(|s| s.as_ref()))
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn planar_differences_round_trip_extremes() {
        let interleaved = [0, i16::MAX, i16::MIN, -1, i16::MAX, i16::MIN, 7, 7];
        let data = SoundData::encode(48_000, 2, &interleaved).unwrap();
        assert_eq!(data.frames, 4);
        assert_eq!(data.samples(), interleaved);
        let copy: SoundData = crate::bin::from_slice(&crate::bin::to_vec(&data)).unwrap();
        assert_eq!(copy, data);
        assert_eq!(copy.samples(), interleaved);
    }
    #[test]
    fn records_refuse_by_reason() {
        let bad = |d: SoundData| d.validate().unwrap_err();
        let ok = SoundData::encode(8_000, 1, &[1, 2, 3]).unwrap();
        assert!(bad(SoundData {
            channels: 3,
            ..ok.clone()
        })
        .contains("mono or stereo"));
        assert!(bad(SoundData {
            rate: 7_999,
            ..ok.clone()
        })
        .contains("outside"));
        assert!(bad(SoundData {
            frames: 4,
            ..ok.clone()
        })
        .contains("frames × channels"));
        assert!(bad(SoundData {
            frames: 0,
            deltas: vec![],
            ..ok.clone()
        })
        .contains("no frames"));
        let huge = SoundData {
            frames: (SOUND_BYTE_BUDGET / 2 + 1) as u32,
            ..ok
        };
        assert!(bad(huge).contains("exceeds the 32.0 MiB sound residency budget"));
    }
}
