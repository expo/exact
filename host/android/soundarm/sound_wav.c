// Decode only Contract's admitted PCM WAV formats, off the render thread.
#include "sound_wav.h"
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static uint16_t u16(const unsigned char *p) { return p[0] | (uint16_t)p[1] << 8; }
static uint32_t u32(const unsigned char *p) { return u16(p) | (uint32_t)u16(p + 2) << 16; }
static float sample(const unsigned char *p, uint16_t format) {
    if (format == 1) return (int16_t)u16(p) / 32768.0f;
    uint32_t bits = u32(p);
    float f;
    memcpy(&f, &bits, 4);
    return isfinite(f) ? f : 0;
}

float *exact_sound_read_wav(const char *path, double rate, uint32_t *frames, uint32_t *channels, char *reason, size_t size) {
    FILE *file = fopen(path, "rb");
    unsigned char *bytes = NULL;
    float *planes = NULL;
    const char *error = "cannot read WAV";
    if (!file) goto done;
    if (fseek(file, 0, SEEK_END)) goto done;
    long length = ftell(file);
    if (length < 12 || length > 16 * 1024 * 1024 || fseek(file, 0, SEEK_SET)) goto done;
    bytes = malloc((size_t)length);
    if (!bytes || fread(bytes, 1, (size_t)length, file) != (size_t)length) goto done;
    error = "unsupported or malformed PCM WAV";
    if (memcmp(bytes, "RIFF", 4) || memcmp(bytes + 8, "WAVE", 4)) goto done;
    size_t end = (size_t)u32(bytes + 4) + 8;
    if (end > (size_t)length || end < 12) goto done;
    const unsigned char *fmt = NULL, *data = NULL;
    uint32_t fmt_size = 0, data_size = 0;
    for (size_t at = 12; at + 8 <= end;) {
        uint32_t n = u32(bytes + at + 4);
        if (n > end - at - 8) goto done;
        if (!memcmp(bytes + at, "fmt ", 4)) { fmt = bytes + at + 8; fmt_size = n; }
        if (!memcmp(bytes + at, "data", 4)) { data = bytes + at + 8; data_size = n; }
        at += 8 + (size_t)n + (n & 1);
    }
    if (!fmt || fmt_size < 16 || !data) goto done;
    uint16_t format = u16(fmt), ch = u16(fmt + 2), block = u16(fmt + 12), bits = u16(fmt + 14);
    uint32_t source_rate = u32(fmt + 4);
    if (format == 0xfffe) {
        static const unsigned char guid_tail[] = {0, 0, 0x10, 0, 0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71};
        if (fmt_size < 40 || u16(fmt + 16) < 22 || u16(fmt + 18) != bits || memcmp(fmt + 28, guid_tail, 12)) goto done;
        uint32_t sub = u32(fmt + 24);
        if (sub != 1 && sub != 3) goto done;
        format = (uint16_t)sub;
    }
    if (!((format == 1 && bits == 16) || (format == 3 && bits == 32)) ||
        (ch != 1 && ch != 2) || source_rate < 8000 || source_rate > 96000 ||
        block != ch * (bits / 8) || !data_size || data_size % block) goto done;
    uint32_t count = data_size / block;
    if (count > source_rate * 10 || rate < 8000 || rate > 192000) goto done;
    *frames = (uint32_t)ceil(count * rate / source_rate);
    *channels = ch;
    planes = malloc(2 * (size_t)*frames * sizeof(float));
    if (!planes) { error = "no memory for decoded WAV"; goto done; }
    for (uint32_t c = 0; c < 2; c++) for (uint32_t i = 0; i < *frames; i++) {
        double at = i * (double)source_rate / rate;
        uint32_t k = (uint32_t)at, channel = ch == 1 ? 0 : c;
        float a = sample(data + k * block + channel * (bits / 8), format);
        float b = k + 1 < count ? sample(data + (k + 1) * block + channel * (bits / 8), format) : 0;
        planes[c * *frames + i] = a + (b - a) * (float)(at - k);
    }
done:
    if (file) fclose(file);
    free(bytes);
    if (!planes && reason && size) snprintf(reason, size, "%s: %s", path, error);
    return planes;
}
