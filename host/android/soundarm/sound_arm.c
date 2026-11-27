// AAudio owns the realtime callback; JNI open/resume/pause/close run on SoundOutput's serial executor.
#include "sound_wav.h"
#include "sound_mixer.h"
#include <aaudio/AAudio.h>
#include <jni.h>
#include <math.h>
#include <stdatomic.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#define JNI(name) Java_dev_exact_canvas_SoundNative_##name

typedef struct {
    AAudioStream *stream;
    ExactMixer *mixer;
    float **sounds;
    uint32_t *sound_frames;
    uint32_t *sound_channels;
    uint32_t count;
    int32_t rate;
    _Atomic int32_t error;
    _Atomic double latency;
    _Atomic bool timestamp;
    double first_callback;
    int64_t stamp_frame, stamp_nanos;
} Arm;

static double seconds(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return (double)t.tv_sec + t.tv_nsec / 1e9;
}

static aaudio_data_callback_result_t render(AAudioStream *stream, void *context, void *data, int32_t frames) {
    Arm *arm = context;
    int64_t frame, nanos;
    double now = seconds();
    double speaker;
    // AAudio timestamps refer to the speaker, so no extra latency is added.
    // getFramesWritten is the first frame of this callback, before AAudio
    // advances it by the buffer returned here.
    int64_t written = AAudioStream_getFramesWritten(stream);
    if (AAudioStream_getTimestamp(stream, CLOCK_MONOTONIC, &frame, &nanos) == AAUDIO_OK) {
        arm->stamp_frame = frame;
        arm->stamp_nanos = nanos;
        speaker = nanos / 1e9 + (double)(written - frame) / arm->rate;
        atomic_store(&arm->timestamp, true);
        atomic_store(&arm->latency, fmax(0, speaker - now));
    } else if (arm->stamp_nanos) {
        speaker = arm->stamp_nanos / 1e9 + (double)(written - arm->stamp_frame) / arm->rate;
        atomic_store(&arm->latency, fmax(0, speaker - now));
    } else {
        if (!arm->first_callback) arm->first_callback = now;
        // Startup has no timestamp yet. Render silence briefly while it
        // arrives; a device that never supplies one uses its queued frames.
        if (now - arm->first_callback < 0.1) {
            memset(data, 0, (size_t)frames * 2 * sizeof(float));
            return AAUDIO_CALLBACK_RESULT_CONTINUE;
        }
        double latency = (double)AAudioStream_getBufferSizeInFrames(stream) / arm->rate;
        speaker = now + latency;
        atomic_store(&arm->timestamp, false);
        atomic_store(&arm->latency, latency);
    }
    float *planes[] = { data };
    uint32_t bytes[] = { (uint32_t)frames * 2 * sizeof(float) };
    ExactSoundBuffers buffers = { 1, planes, bytes };
    exact_sound_mix(arm->mixer, speaker, (uint32_t)frames, &buffers);
    return AAUDIO_CALLBACK_RESULT_CONTINUE;
}

static void failed(AAudioStream *stream, void *context, aaudio_result_t error) {
    (void)stream;
    // Closing from an AAudio callback is forbidden. The main-thread owner
    // reads the error, closes the stream and retries at a later play.
    atomic_store(&((Arm *)context)->error, error);
}

static void pause_stream(Arm *arm) {
    if (!arm) return;
    if (arm->stream) {
        AAudioStream_requestStop(arm->stream);
        AAudioStream_close(arm->stream);
        arm->stream = NULL;
    }
    arm->first_callback = 0;
    arm->stamp_frame = 0;
    arm->stamp_nanos = 0;
    atomic_store(&arm->error, 0);
    atomic_store(&arm->latency, 0);
    atomic_store(&arm->timestamp, false);
}

static void free_sounds(Arm *arm) {
    if (!arm) return;
    exact_sound_mixer_free(arm->mixer);
    arm->mixer = NULL;
    for (uint32_t i = 0; i < arm->count; i++) free(arm->sounds[i]);
    free(arm->sounds);
    free(arm->sound_frames);
    free(arm->sound_channels);
    arm->sounds = NULL;
    arm->sound_frames = NULL;
    arm->sound_channels = NULL;
    arm->count = 0;
}

static void close_arm(Arm *arm) {
    if (!arm) return;
    pause_stream(arm);
    free_sounds(arm);
    free(arm);
}

static void throw_error(JNIEnv *env, const char *reason) {
    jclass type = (*env)->FindClass(env, "java/lang/IllegalStateException");
    if (type) (*env)->ThrowNew(env, type, reason);
}

static char *file_path(JNIEnv *env, jstring file) {
    // JNI's GetStringUTFChars uses modified UTF-8: supplementary characters
    // become surrogate pairs, which are not the filesystem's UTF-8 bytes.
    jclass string = (*env)->GetObjectClass(env, file);
    if (!string) return NULL;
    jmethodID encode = (*env)->GetMethodID(env, string, "getBytes", "(Ljava/lang/String;)[B");
    (*env)->DeleteLocalRef(env, string);
    if (!encode) return NULL;
    jstring utf8 = (*env)->NewStringUTF(env, "UTF-8");
    if (!utf8) return NULL;
    jbyteArray bytes = (*env)->CallObjectMethod(env, file, encode, utf8);
    (*env)->DeleteLocalRef(env, utf8);
    if ((*env)->ExceptionCheck(env) || !bytes) return NULL;
    jsize length = (*env)->GetArrayLength(env, bytes);
    char *path = calloc((size_t)length + 1, 1);
    if (path) {
        (*env)->GetByteArrayRegion(env, bytes, 0, length, (jbyte *)path);
        if ((*env)->ExceptionCheck(env)) { free(path); path = NULL; }
    }
    (*env)->DeleteLocalRef(env, bytes);
    return path;
}

static bool open_stream(Arm *arm, jint usage, jint content, int32_t *out_rate, char *reason, size_t reason_size) {
    AAudioStreamBuilder *builder = NULL;
    aaudio_result_t result = AAudio_createStreamBuilder(&builder);
    if (result != AAUDIO_OK) {
        snprintf(reason, reason_size, "AAudio: %s", AAudio_convertResultToText(result));
        return false;
    }
    AAudioStreamBuilder_setDirection(builder, AAUDIO_DIRECTION_OUTPUT);
    AAudioStreamBuilder_setFormat(builder, AAUDIO_FORMAT_PCM_FLOAT);
    AAudioStreamBuilder_setChannelCount(builder, 2);
    AAudioStreamBuilder_setSharingMode(builder, AAUDIO_SHARING_MODE_SHARED);
    AAudioStreamBuilder_setPerformanceMode(builder, AAUDIO_PERFORMANCE_MODE_LOW_LATENCY);
    AAudioStreamBuilder_setUsage(builder, usage);
    AAudioStreamBuilder_setContentType(builder, content);
    AAudioStreamBuilder_setDataCallback(builder, render, arm);
    AAudioStreamBuilder_setErrorCallback(builder, failed, arm);
    result = AAudioStreamBuilder_openStream(builder, &arm->stream);
    AAudioStreamBuilder_delete(builder);
    if (result != AAUDIO_OK) {
        snprintf(reason, reason_size, "AAudio: %s", AAudio_convertResultToText(result));
        return false;
    }
    int32_t rate = AAudioStream_getSampleRate(arm->stream);
    if (AAudioStream_getFormat(arm->stream) != AAUDIO_FORMAT_PCM_FLOAT ||
        AAudioStream_getChannelCount(arm->stream) != 2 || rate <= 0) {
        snprintf(reason, reason_size, "AAudio did not provide stereo float output");
        pause_stream(arm);
        return false;
    }
    *out_rate = rate;
    return true;
}

static bool decode_files(JNIEnv *env, Arm *arm, jobjectArray files, jbooleanArray decoded, int32_t rate, char *reason, size_t reason_size) {
    free_sounds(arm);
    arm->rate = rate;
    arm->mixer = exact_sound_mixer(2, rate);
    if (!arm->mixer) return false;
    uint32_t count = (uint32_t)(*env)->GetArrayLength(env, files);
    if (count > 0) {
        arm->sounds = calloc(count, sizeof(float *));
        arm->sound_frames = calloc(count, sizeof(uint32_t));
        arm->sound_channels = calloc(count, sizeof(uint32_t));
        if (!arm->sounds || !arm->sound_frames || !arm->sound_channels) {
            free_sounds(arm);
            return false;
        }
    }
    arm->count = count;
    for (uint32_t i = 0; i < count; i++) {
        jstring file = (*env)->GetObjectArrayElement(env, files, (jsize)i);
        if (!file) return false;
        char *path = file_path(env, file);
        if (!path) { (*env)->DeleteLocalRef(env, file); return false; }
        uint32_t frames = 0, channels = 0;
        arm->sounds[i] = exact_sound_read_wav(path, rate, &frames, &channels, reason, reason_size);
        free(path);
        (*env)->DeleteLocalRef(env, file);
        jboolean ok = arm->sounds[i] != NULL;
        (*env)->SetBooleanArrayRegion(env, decoded, (jsize)i, 1, &ok);
        if ((*env)->ExceptionCheck(env)) return false;
        if (!ok) continue;
        arm->sound_frames[i] = frames;
        arm->sound_channels[i] = channels;
        exact_sound_set(arm->mixer, i, arm->sounds[i], frames, channels);
    }
    return true;
}

static bool rebind_cached(JNIEnv *env, Arm *arm, jbooleanArray decoded) {
    ExactMixer *fresh = exact_sound_mixer(2, arm->rate);
    if (!fresh) return false;
    exact_sound_mixer_free(arm->mixer);
    arm->mixer = fresh;
    for (uint32_t i = 0; i < arm->count; i++) {
        jboolean ok = arm->sounds[i] != NULL;
        (*env)->SetBooleanArrayRegion(env, decoded, (jsize)i, 1, &ok);
        if ((*env)->ExceptionCheck(env)) return false;
        if (ok) exact_sound_set(arm->mixer, i, arm->sounds[i], arm->sound_frames[i], arm->sound_channels[i]);
    }
    return true;
}

JNIEXPORT jlong JNICALL JNI(open)(JNIEnv *env, jclass klass, jobjectArray files, jint usage, jint content, jbooleanArray decoded) {
    (void)klass;
    Arm *arm = calloc(1, sizeof(Arm));
    char reason[512] = "no memory for sound output";
    int32_t rate = 0;
    if (!arm) goto fail;
    if (!open_stream(arm, usage, content, &rate, reason, sizeof(reason))) goto fail;
    if (!decode_files(env, arm, files, decoded, rate, reason, sizeof(reason))) goto fail;
    aaudio_result_t result = AAudioStream_requestStart(arm->stream);
    if (result != AAUDIO_OK) {
        snprintf(reason, sizeof(reason), "AAudio: %s", AAudio_convertResultToText(result));
        goto fail;
    }
    return (jlong)(intptr_t)arm;
fail:
    close_arm(arm);
    if (!(*env)->ExceptionCheck(env)) throw_error(env, reason);
    return 0;
}

JNIEXPORT jboolean JNICALL JNI(resume)(JNIEnv *env, jclass klass, jlong handle, jobjectArray files, jint usage, jint content, jbooleanArray decoded) {
    (void)klass;
    Arm *arm = (Arm *)(intptr_t)handle;
    char reason[512] = "no memory for sound output";
    if (!arm) { throw_error(env, "sound output is closed"); return JNI_FALSE; }
    pause_stream(arm);
    int32_t rate = 0;
    if (!open_stream(arm, usage, content, &rate, reason, sizeof(reason))) goto fail;
    uint32_t count = (uint32_t)(*env)->GetArrayLength(env, files);
    bool ok = (rate == arm->rate && count == arm->count && arm->mixer != NULL)
        ? rebind_cached(env, arm, decoded)
        : decode_files(env, arm, files, decoded, rate, reason, sizeof(reason));
    if (!ok) goto fail;
    aaudio_result_t result = AAudioStream_requestStart(arm->stream);
    if (result != AAUDIO_OK) {
        snprintf(reason, sizeof(reason), "AAudio: %s", AAudio_convertResultToText(result));
        goto fail;
    }
    return JNI_TRUE;
fail:
    pause_stream(arm);
    if (!(*env)->ExceptionCheck(env)) throw_error(env, reason);
    return JNI_FALSE;
}

JNIEXPORT void JNICALL JNI(pause)(JNIEnv *env, jclass klass, jlong handle) {
    (void)env; (void)klass;
    pause_stream((Arm *)(intptr_t)handle);
}

JNIEXPORT jboolean JNICALL JNI(op)(JNIEnv *env, jclass klass, jlong handle, jint kind, jlong id, jint sound, jdouble at, jfloat gain, jfloat rate, jfloat pan, jdouble ramp) {
    (void)env; (void)klass;
    Arm *arm = (Arm *)(intptr_t)handle;
    if (!arm || !arm->mixer || !exact_sound_room(arm->mixer)) return JNI_FALSE;
    bool pushed = kind == 4 ? exact_sound_change(arm->mixer, id, at, gain, rate, pan, ramp)
                            : exact_sound_push(arm->mixer, kind, id, sound, at, gain, rate, pan);
    return pushed ? JNI_TRUE : JNI_FALSE;
}

JNIEXPORT jdoubleArray JNICALL JNI(counts)(JNIEnv *env, jclass klass, jlong handle) {
    (void)klass;
    Arm *arm = (Arm *)(intptr_t)handle;
    double values[] = {
        exact_sound_live(arm->mixer), exact_sound_dropped(arm->mixer), exact_sound_late(arm->mixer),
        atomic_load(&arm->latency), arm->rate, atomic_load(&arm->timestamp) ? 1 : 0,
        atomic_load(&arm->error), EXACT_SOUND_RING - exact_sound_room(arm->mixer),
        arm->stream ? AAudioStream_getXRunCount(arm->stream) : 0,
    };
    jdoubleArray out = (*env)->NewDoubleArray(env, 9);
    if (out) (*env)->SetDoubleArrayRegion(env, out, 0, 9, values);
    return out;
}

JNIEXPORT void JNICALL JNI(close)(JNIEnv *env, jclass klass, jlong handle) {
    (void)env; (void)klass;
    close_arm((Arm *)(intptr_t)handle);
}
