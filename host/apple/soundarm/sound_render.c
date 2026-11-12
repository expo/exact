// Apple's source node uses mach host time and AudioBufferList; the mixer is shared.
#include "sound_render.h"
#include <mach/mach_time.h>

/// Host ticks (mach_absolute_time, CACurrentMediaTime's base) as seconds.
double exact_sound_seconds(uint64_t host) {
    static mach_timebase_info_data_t base;
    if (base.denom == 0) mach_timebase_info(&base);
    return (double)host * base.numer / base.denom / 1e9;
}

bool exact_sound_render(ExactMixer *m, const AudioTimeStamp *ts, uint32_t frames, AudioBufferList *out) {
    float *planes[out->mNumberBuffers];
    uint32_t bytes[out->mNumberBuffers];
    for (uint32_t b = 0; b < out->mNumberBuffers; b++) {
        planes[b] = out->mBuffers[b].mData;
        bytes[b] = out->mBuffers[b].mDataByteSize;
    }
    ExactSoundBuffers buffers = { out->mNumberBuffers, planes, bytes };
    double rendered = (ts && (ts->mFlags & kAudioTimeStampHostTimeValid)) ? exact_sound_seconds(ts->mHostTime) : 0;
    return exact_sound_mix(m, rendered, frames, &buffers);
}
