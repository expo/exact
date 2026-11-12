// @ref LLP 1096 D6, D8, LLP 1125 D10. See sound_mixer.h.
#include "sound_mixer.h"
#include <math.h>
#include <stdatomic.h>
#include <stdlib.h>
#include <string.h>

typedef struct { uint32_t kind; uint32_t sound; uint64_t id; double at; float gain, rate, pan; double ramp; } Op;

/// Changes a voice holds before their time: more in one buffer than a
/// commit at every frame makes.
#define EXACT_SOUND_PENDING 8

/// A value moving linearly from `from` at `t0` to `to` at `t1`, host seconds; `to` after.
typedef struct { double from, to, t0, t1; } Ramp;

static inline double along(const Ramp *r, double t) {
    if (t >= r->t1) return r->to;
    if (t <= r->t0) return r->from;
    return r->from + (r->to - r->from) * (t - r->t0) / (r->t1 - r->t0);
}

typedef struct {
    bool live, started;
    uint64_t id;
    uint32_t sound;
    double start, end;   // host seconds at the speaker; `end` infinite until an End
    Ramp gain;
    double position;     // where in its sound the next frame reads, in frames
    Ramp rate;           // frames of its sound a frame of output
    bool panned;
    double panFrom;      // when it is panned from: a change can pan one that was not
    Ramp pan;
    float left, right;   // the equal-power gains of the pan once its ramp is over
    // Changes not yet reached, in time order: each takes effect at the frame
    // its time reaches, so a later one never rewrites an earlier one's ramp.
    Op pending[EXACT_SOUND_PENDING];
    uint32_t waiting;
} Voice;

typedef struct { const float *planes; uint32_t frames, source; } Sound;

struct ExactMixer {
    uint32_t channels;
    double rate;
    _Atomic double latency;
    Op ring[EXACT_SOUND_RING];
    _Atomic uint32_t head, tail;
    Voice slots[EXACT_SOUND_SLOTS];
    Sound *sounds;
    uint32_t count;
    _Atomic uint64_t dropped, late;
    _Atomic uint32_t live;
};

ExactMixer *exact_sound_mixer(uint32_t channels, double rate) {
    ExactMixer *m = calloc(1, sizeof(ExactMixer));
    if (!m) return NULL;
    m->channels = channels;
    m->rate = rate;
    return m;
}

void exact_sound_mixer_free(ExactMixer *m) {
    if (!m) return;
    free(m->sounds);
    free(m);
}

void exact_sound_set(ExactMixer *m, uint32_t index, const float *planes, uint32_t frames, uint32_t source) {
    // Called before the engine starts (or while it is stopped): not on the render thread.
    if (index >= m->count) {
        Sound *grown = realloc(m->sounds, (index + 1) * sizeof(Sound));
        if (!grown) return;
        memset(grown + m->count, 0, (index + 1 - m->count) * sizeof(Sound));
        m->sounds = grown;
        m->count = index + 1;
    }
    m->sounds[index] = (Sound){ planes, frames, source };
}

static bool push(ExactMixer *m, Op op) {
    uint32_t tail = atomic_load_explicit(&m->tail, memory_order_relaxed);
    uint32_t head = atomic_load_explicit(&m->head, memory_order_acquire);
    if (tail - head >= EXACT_SOUND_RING) {
        atomic_fetch_add_explicit(&m->dropped, 1, memory_order_relaxed);
        return false;
    }
    m->ring[tail % EXACT_SOUND_RING] = op;
    atomic_store_explicit(&m->tail, tail + 1, memory_order_release);
    return true;
}

bool exact_sound_push(ExactMixer *m, uint32_t kind, uint64_t id, uint32_t sound, double at, float gain, float rate, float pan) {
    return push(m, (Op){ kind, sound, id, at, gain, rate, pan, 0 });
}

bool exact_sound_change(ExactMixer *m, uint64_t id, double at, float gain, float rate, float pan, double ramp) {
    return push(m, (Op){ 4, 0, id, at, gain, rate, pan, isfinite(ramp) && ramp > 0 ? ramp : 0 });
}

void exact_sound_latency(ExactMixer *m, double seconds) { atomic_store_explicit(&m->latency, seconds, memory_order_relaxed); }
uint64_t exact_sound_dropped(const ExactMixer *m) { return atomic_load_explicit(&m->dropped, memory_order_relaxed); }
uint64_t exact_sound_late(const ExactMixer *m) { return atomic_load_explicit(&m->late, memory_order_relaxed); }
uint32_t exact_sound_room(const ExactMixer *m) {
    uint32_t head = atomic_load_explicit(&m->head, memory_order_acquire);
    uint32_t tail = atomic_load_explicit(&m->tail, memory_order_relaxed);
    return EXACT_SOUND_RING - (tail - head);
}
uint32_t exact_sound_live(const ExactMixer *m) { return atomic_load_explicit(&m->live, memory_order_relaxed); }

/// Whether a slot's voice is over by `speaker`, the buffer's speaker time:
/// it ends before this buffer (started or not), or was cut before it
/// sounded (an End this apply took counts), so a Play behind it can take
/// its slot now.
static bool over(const Voice *v, double speaker) {
    return !v->live || v->end <= speaker || (!v->started && v->end <= v->start);
}

/// Plane `c` of `sound` between frame `k` and `k + 1`, `fraction` of the way.
static inline float sample(const Sound *sound, uint32_t c, uint32_t k, float fraction) {
    const float *plane = sound->planes + (size_t)c * sound->frames;
    float a = plane[k], b = k + 1 < sound->frames ? plane[k + 1] : 0;
    return a + (b - a) * fraction;
}

/// Web Audio 1.1, StereoPannerNode: a mono input's x is (pan + 1) / 2; a stereo
/// input's is pan + 1 left of centre and pan right of it, its far side kept whole.
static inline void place(const Sound *sound, double pan, float *left, float *right) {
    double x = sound->source >= 2 ? (pan <= 0 ? pan + 1 : pan) : (pan + 1) / 2;
    *left = (float)cos(x * M_PI / 2);
    *right = (float)sin(x * M_PI / 2);
}

/// Change `op` to voice `v`: from `op.at` each value given moves from where
/// its ramp has it then, or, for a voice whose start is after the change,
/// takes the value outright.
static void change(ExactMixer *m, Voice *v, const Op *op) {
    bool outright = op->at < v->start;
    float given[3] = { op->gain, op->rate, op->pan };
    Ramp *ramps[3] = { &v->gain, &v->rate, &v->pan };
    if (isfinite(op->pan) && op->pan != 0 && !v->panned) {
        v->panned = true;
        v->panFrom = outright ? -INFINITY : op->at;
        v->pan = (Ramp){ 0, 0, v->start, v->start };
    }
    for (int p = 0; p < 3; p++) {
        if (!isfinite(given[p]) || (p == 2 && !v->panned)) continue;
        double to = p == 1 ? (given[p] > 0 ? given[p] : 1) : p == 2 ? fmin(fmax(given[p], -1), 1) : given[p];
        Ramp *r = ramps[p];
        *r = outright ? (Ramp){ to, to, v->start, v->start } : (Ramp){ along(r, op->at), to, op->at, op->at + op->ramp };
    }
    if (v->panned) place(&m->sounds[v->sound], v->pan.to, &v->left, &v->right);
}

static inline double ramps_end(const Voice *v) {
    return fmax(v->gain.t1, fmax(v->rate.t1, v->panned ? v->pan.t1 : 0));
}

/// Mix voice `v` into `out` for this buffer of `frames` frames, whose first
/// reaches the speaker at `speaker`: from its first frame (or where it is)
/// to its end, the file's or an End's. Whether it is still live after it.
static bool mix(ExactMixer *m, Voice *v, double speaker, uint32_t frames, ExactSoundBuffers *out, bool interleaved, bool *sounded) {
    const Sound *sound = &m->sounds[v->sound];
    int64_t begin = 0;
    if (!v->started) {
        int64_t first = llround((v->start - speaker) * m->rate);
        if (first >= (int64_t)frames) return true;
        // A voice the mapping puts in the past starts here, from its first
        // frame: its attack is never skipped.
        if (first < 0) { first = 0; atomic_fetch_add_explicit(&m->late, 1, memory_order_relaxed); }
        begin = first;
    }
    int64_t stop = frames;
    if (isfinite(v->end)) {
        int64_t endFrame = llround((v->end - speaker) * m->rate);
        if (endFrame < stop) stop = endFrame;
    }
    // Cut before it sounded here, or ended in an earlier buffer.
    if (stop <= begin) { v->live = false; return false; }
    v->started = true;
    // A stereo sound for a mono output keeps both its planes (exact_sound_set).
    uint32_t stored = m->channels == 1 && sound->source >= 2 ? 2 : m->channels;
    // Frame i reaches the speaker at speaker + i / rate; a ramp is read there.
    double rampsEnd = ramps_end(v);
    for (int64_t i = begin; i < stop && v->position < sound->frames; i++) {
        double at = speaker + (double)i / m->rate;
        if (v->waiting && v->pending[0].at <= at) {
            uint32_t n = 0;
            while (n < v->waiting && v->pending[n].at <= at) change(m, v, &v->pending[n++]);
            memmove(v->pending, v->pending + n, (v->waiting - n) * sizeof(Op));
            v->waiting -= n;
            rampsEnd = ramps_end(v);
        }
        bool ramping = at < rampsEnd, panned = v->panned && at >= v->panFrom;
        float gain = ramping ? (float)along(&v->gain, at) : (float)v->gain.to;
        double pan = !panned ? 0 : ramping ? along(&v->pan, at) : v->pan.to;
        float left = v->left, right = v->right;
        if (panned && ramping && at < v->pan.t1) place(sound, pan, &left, &right);
        // Between frame k and k + 1 by linear interpolation; past the last frame is silence.
        // At the file's own rate the fraction is 0, and the frame is read as it is.
        uint32_t k = (uint32_t)v->position;
        float fraction = (float)(v->position - k);
        v->position += ramping ? along(&v->rate, at) : v->rate.to;
        if (!panned && stored == m->channels) {
            for (uint32_t c = 0; c < m->channels; c++) {
                float x = sample(sound, c, k, fraction) * gain;
                if (interleaved) out->data[0][i * m->channels + c] += x;
                else if (c < out->count) out->data[c][i] += x;
            }
            *sounded = true;
            continue;
        }
        // A mono sound's planes are copies of it, so its left is all of it.
        float l = sample(sound, 0, k, fraction), r = stored > 1 ? sample(sound, 1, k, fraction) : l;
        float outL = l, outR = r;
        if (panned) {
            if (sound->source < 2) { outL = l * left; outR = l * right; }
            else if (pan <= 0) { outL = l + r * left; outR = r * right; }
            else { outL = l * left; outR = r + l * right; }
        }
        if (m->channels == 1) {
            // Web Audio 1.1's speaker down-mix of stereo to mono: 0.5 × (L + R).
            out->data[0][i] += 0.5f * (outL + outR) * gain;
        } else if (interleaved) {
            // The channels past the second take nothing, as a destination up-mixing the
            // panner's stereo output gives them.
            out->data[0][i * m->channels] += outL * gain;
            out->data[0][i * m->channels + 1] += outR * gain;
        } else {
            out->data[0][i] += outL * gain;
            if (out->count > 1) out->data[1][i] += outR * gain;
        }
        *sounded = true;
    }
    // A slot frees at its end: the file's, or an End's.
    if (v->position >= sound->frames || stop < (int64_t)frames) { v->live = false; return false; }
    return true;
}

/// The last end queued for a voice before the next stop-all; whether that stop-all silences it.
static bool pending_end(ExactMixer *m, Voice *v, uint32_t next, uint32_t tail) {
    for (uint32_t j = next; j != tail; j++) {
        const Op *later = &m->ring[j % EXACT_SOUND_RING];
        if (later->kind == 3) return true;
        if (later->kind == 2 && later->id == v->id) v->end = later->at;
    }
    return false;
}

/// A slot whose voice ends inside this buffer (an End's or before), its
/// remaining frames mixed now, so a Play can take it: mixing is a sum, so
/// a voice mixed early sounds as it would have. Retriggers that cut voices
/// within one buffer, however many rounds, then need no slots beyond the
/// runner's bounds.
///
/// The ops still behind the Play in this apply, `[next, tail)`, are read
/// first: the last End there replaces its end before it is mixed or its slot
/// reused, and a stop-all there means it never sounds in this buffer. The
/// scan stops at that stop-all, since ids after it are the next boot's.
static Voice *steal(ExactMixer *m, double speaker, uint32_t frames, ExactSoundBuffers *out, bool interleaved, bool *sounded,
                    uint32_t next, uint32_t tail) {
    for (int i = 0; i < EXACT_SOUND_SLOTS; i++) {
        Voice *v = &m->slots[i];
        if (!v->live) continue;
        bool silenced = pending_end(m, v, next, tail);
        if (!silenced) {
            if (!isfinite(v->end) || llround((v->end - speaker) * m->rate) > (int64_t)frames) continue;
            mix(m, v, speaker, frames, out, interleaved, sounded);
        }
        v->live = false;
        return v;
    }
    return NULL;
}

/// Every op in the ring, in order: a Play and an End for the same buffer both take effect in it.
/// After the output is zeroed: a steal mixes into it.
static void apply(ExactMixer *m, double speaker, uint32_t frames, ExactSoundBuffers *out, bool interleaved, bool *sounded) {
    uint32_t head = atomic_load_explicit(&m->head, memory_order_relaxed);
    uint32_t tail = atomic_load_explicit(&m->tail, memory_order_acquire);
    for (; head != tail; head++) {
        Op op = m->ring[head % EXACT_SOUND_RING];
        if (op.kind == 1) {
            Voice *free_slot = NULL;
            for (int i = 0; i < EXACT_SOUND_SLOTS && !free_slot; i++) {
                Voice *v = &m->slots[i];
                if (v->live && pending_end(m, v, head + 1, tail)) v->live = false;
                if (over(v, speaker)) free_slot = v;
            }
            if (!free_slot) free_slot = steal(m, speaker, frames, out, interleaved, sounded, head + 1, tail);
            if (!free_slot || op.sound >= m->count || !m->sounds[op.sound].planes) {
                atomic_fetch_add_explicit(&m->dropped, 1, memory_order_relaxed);
                continue;
            }
            double rate = isfinite(op.rate) && op.rate > 0 ? op.rate : 1;
            Voice v = { .live = true, .id = op.id, .sound = op.sound, .start = op.at, .end = INFINITY,
                        .gain = { op.gain, op.gain, op.at, op.at }, .rate = { rate, rate, op.at, op.at } };
            // A pan of 0 leaves the voice as it was: no panner, its level unchanged.
            if (isfinite(op.pan) && op.pan != 0) {
                double pan = fmin(fmax(op.pan, -1), 1);
                v.panned = true;
                v.panFrom = -INFINITY;
                v.pan = (Ramp){ pan, pan, op.at, op.at };
                place(&m->sounds[op.sound], pan, &v.left, &v.right);
            }
            *free_slot = v;
        } else if (op.kind == 2) {
            for (int i = 0; i < EXACT_SOUND_SLOTS; i++) {
                Voice *v = &m->slots[i];
                if (v->live && v->id == op.id) v->end = op.at;
                // Retracted before it sounded (an edit, a stop): its slot is free now, not at its start.
                if (v->live && v->id == op.id && !v->started && op.at <= v->start) v->live = false;
            }
        } else if (op.kind == 3) {
            for (int i = 0; i < EXACT_SOUND_SLOTS; i++) m->slots[i].live = false;
        } else if (op.kind == 4) {
            for (int i = 0; i < EXACT_SOUND_SLOTS; i++) {
                Voice *v = &m->slots[i];
                if (!v->live || v->id != op.id) continue;
                // One whose start is after the change takes the values now; any
                // other waits for the frame its time reaches. A full queue lets
                // its oldest take effect early.
                if (op.at < v->start) { change(m, v, &op); continue; }
                if (v->waiting == EXACT_SOUND_PENDING) {
                    change(m, v, &v->pending[0]);
                    memmove(v->pending, v->pending + 1, (EXACT_SOUND_PENDING - 1) * sizeof(Op));
                    v->waiting--;
                }
                v->pending[v->waiting++] = op;
            }
        }
    }
    atomic_store_explicit(&m->head, head, memory_order_release);
}

bool exact_sound_mix(ExactMixer *m, double rendered, uint32_t frames, ExactSoundBuffers *out) {
    bool interleaved = out->count == 1 && m->channels > 1;
    for (uint32_t b = 0; b < out->count; b++)
        memset(out->data[b], 0, out->bytes[b]);
    double speaker = rendered + atomic_load_explicit(&m->latency, memory_order_relaxed);
    bool sounded = false;
    apply(m, speaker, frames, out, interleaved, &sounded);
    uint32_t live = 0;
    for (int s = 0; s < EXACT_SOUND_SLOTS; s++) {
        Voice *v = &m->slots[s];
        if (v->live && mix(m, v, speaker, frames, out, interleaved, &sounded)) live++;
    }
    atomic_store_explicit(&m->live, live, memory_order_relaxed);
    return sounded;
}
