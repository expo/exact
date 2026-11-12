// Apple's clock and buffer adapter for the shared sound mixer.
#ifndef EXACT_SOUND_RENDER_H
#define EXACT_SOUND_RENDER_H
#include "sound_mixer.h"
#include <CoreAudioTypes/CoreAudioTypes.h>

/// Host ticks (`mHostTime`, mach_absolute_time) as seconds on
/// CACurrentMediaTime's timebase.
double exact_sound_seconds(uint64_t host);
/// Fill `out` with `frames` frames: the render callback. False when every
/// frame is silence (the node's `isSilence`).
bool exact_sound_render(ExactMixer *mixer, const AudioTimeStamp *timestamp, uint32_t frames, AudioBufferList *out);

#ifdef __OBJC__
// The arm's AVAudioEngine calls (sound_engine.m). AVAudioEngine raises
// Objective-C exceptions, rather than throwing, for a graph it cannot make;
// an exception must not unwind through a Swift frame, so each call that can
// raise sits directly inside its own @try here, and a failure comes back as
// nil or false with why in `reason` (`size` bytes, NUL-terminated).
#import <AVFAudio/AVFAudio.h>
#include <stddef.h>
/// A new engine, and its output's rate and channel count.
AVAudioEngine *exact_sound_engine_new(double *rate, uint32_t *channels, char *reason, size_t size);
/// A source node rendering `mixer`, attached and connected to the main mixer
/// in the output's format, and the engine prepared.
AVAudioSourceNode *exact_sound_engine_connect(AVAudioEngine *engine, ExactMixer *mixer, double rate, uint32_t channels, char *reason, size_t size);
bool exact_sound_engine_start(AVAudioEngine *engine, char *reason, size_t size);
bool exact_sound_engine_pause(AVAudioEngine *engine, char *reason, size_t size);
/// False when even stopping raised.
bool exact_sound_engine_stop(AVAudioEngine *engine);
/// Stops and releases an engine and its source node, each a +1 handed over,
/// whatever raises: an orphan after a media services reset (QA1749).
void exact_sound_engine_dispose(void *engine, void *source);
/// A tap on the main mixer's output (the device check).
bool exact_sound_engine_tap(AVAudioEngine *engine, void (^block)(AVAudioPCMBuffer *, AVAudioTime *), char *reason, size_t size);
#endif

#endif
