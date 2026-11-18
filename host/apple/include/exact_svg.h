/* exact_svg.h — the SVG family's C ABI (LLP 1047.001 D4, D10): the SVG
 * files images and icons decode, and the island and filter work a plan
 * needs. Its exports are `exact_apple::svg_exports!`'s; the SVG family's
 * Swift, and only it, calls them. */
#ifndef EXACT_SVG_H
#define EXACT_SVG_H

#include "exact.h"

#ifdef __cplusplus
extern "C" {
#endif

/* An SVG file as an image (LLP 1011.001 D4). header: 1 when bytes begin an SVG,
 * writing its natural size (CSS pixels) into size[0..2]; else 0. scene: the
 * file resolved at width x height CSS pixels as the svg op's JSON, in a buffer
 * of *out_len bytes to free with exact_svg_image_free; *flags bit 0 means the
 * picture has text (drawn on the main thread, where the fonts are), bit 1 that
 * the file was refused, the buffer then holding the reason. */
uint32_t exact_svg_image_header(const uint8_t *bytes, size_t len, double *size);
uint8_t *exact_svg_image_scene(const uint8_t *bytes, size_t len, double width, double height, size_t *out_len, uint32_t *flags);
void exact_svg_image_free(uint8_t *ptr, size_t len);

/* What SVG pixel work the live plan can need, as bits (0 before a boot):
 * 1 an island, a `mask` or a `filter` (LLP 1055.000 §8 ruling 4), when the
 * host should open its island module off the main thread now; 2 a filter,
 * when it should make its GPU filter pipelines now. */
uint8_t exact_svg_islands(ExactRuntime rt);

#ifdef __cplusplus
}
#endif
#endif
