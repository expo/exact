#ifndef EXACT_SOUND_WAV_H
#define EXACT_SOUND_WAV_H
#include <stdint.h>
#include <stddef.h>
// The admitted WAV formats, resampled once into two float planes.
float *exact_sound_read_wav(const char *path, double rate, uint32_t *frames, uint32_t *channels, char *reason, size_t size);
#endif
