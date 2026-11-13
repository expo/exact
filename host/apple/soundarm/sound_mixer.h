// @ref LLP 1096 D8, LLP 1125 D10. The sound arm's mixer: a C render
// callback with no allocation and no lock in it. The host pushes ops onto a
// single-producer, single-consumer ring; each render applies every op in the
// ring, then sums every live voice from its first frame to its end frame,
// times its gain. A voice reads its sound at its playback rate, from a
// fractional position by linear interpolation, and a panned one is placed by
// the Web Audio StereoPannerNode's equal-power algorithm. A change (LLP
// 1096 D14) moves a voice's gain, rate and pan linearly from where they are
// at its time to new values over its ramp, evaluated at every frame. Times
// are host seconds on the host's monotonic clock: when the voice should
// reach the speaker.
#ifndef EXACT_SOUND_MIXER_H
#define EXACT_SOUND_MIXER_H

#include <stdint.h>
#include <stdbool.h>

/// Voices in slots at once: the runner bounds the voices sounding at any
/// start to 32 and those waiting to 256 (LLP 1125 D3). A Play that finds
/// no free slot takes one whose voice ends inside the buffer, that voice's
/// remaining frames mixed first, so retriggers cut within one buffer need
/// no more.
#define EXACT_SOUND_SLOTS 288
/// Ops the ring holds: a second's lookahead of plays, and their ends.
#define EXACT_SOUND_RING 1024

typedef struct ExactMixer ExactMixer;

/// A mixer for `channels` deinterleaved float channels at `rate` frames a second.
ExactMixer *exact_sound_mixer(uint32_t channels, double rate);
void exact_sound_mixer_free(ExactMixer *mixer);
/// Sound `index`'s samples, converted to the output's rate: planes of
/// `frames` floats, one after another, one per output channel, except that a
/// stereo file for a mono output keeps its two (the mixer down-mixes them,
/// after a pan). `source` is the file's own channel count (1 or 2), which
/// decides how a pan places it. The caller keeps the planes alive.
void exact_sound_set(ExactMixer *mixer, uint32_t index, const float *planes, uint32_t frames, uint32_t source);
/// Push an op: kind 1 plays `sound` from `at` times `gain`, read at `rate`
/// (Web Audio's `playbackRate`, 1 the file's own) and placed at `pan` (−1
/// left to 1 right; 0 is the voice as it was, unpanned), on a mono output
/// panned in stereo and then down-mixed as Web Audio does; kind 2 ends voice
/// `id` at `at`, replacing its previous end (a freed slot is ignored; one at or
/// before a voice's start that has not started frees its slot); kind 3 ends
/// every voice now. False when the ring is full: the op is dropped and counted.
bool exact_sound_push(ExactMixer *mixer, uint32_t kind, uint64_t id, uint32_t sound, double at, float gain, float rate, float pan);
/// Push a change to voice `id` (LLP 1096 D14): from `at`, each of `gain`,
/// `rate` and `pan` that is not NaN moves linearly from its value then to the
/// new one, reaching it `ramp` seconds later; a voice whose start is after
/// `at` takes them outright. A voice with no pan that is given one other than
/// 0 is panned from then on, from centred. False when the ring is full, as
/// for `exact_sound_push`.
bool exact_sound_change(ExactMixer *mixer, uint64_t id, double at, float gain, float rate, float pan, double ramp);
/// Ops the ring has room for now: a producer that waits for room never
/// counts a drop.
uint32_t exact_sound_room(const ExactMixer *mixer);
/// The delay from the time supplied to exact_sound_mix to the speaker.
/// Apple supplies its presentation latency; Android supplies speaker time directly.
void exact_sound_latency(ExactMixer *mixer, double seconds);
/// Ops the ring dropped, plays no slot took, and voices started late (their
/// first frame already past), since the mixer was made.
uint64_t exact_sound_dropped(const ExactMixer *mixer);
uint64_t exact_sound_late(const ExactMixer *mixer);
/// Voices in slots now.
uint32_t exact_sound_live(const ExactMixer *mixer);
/// One interleaved buffer, or one float plane per output channel.
typedef struct { uint32_t count; float **data; const uint32_t *bytes; } ExactSoundBuffers;
/// Render at `seconds` on the same timebase as the operations, plus latency.
/// False when every frame is silence. Buffers hold at least `frames` frames.
bool exact_sound_mix(ExactMixer *mixer, double seconds, uint32_t frames, ExactSoundBuffers *out);

#endif
