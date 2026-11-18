/* exact_web.h — the web views' agent operation (LLP 1047.001 D11): the agent operations this
 * family's Swift asks in a production build, each its own export
 * (`exact_apple::answers_exports!`). Each answers as `exact_agent` answers
 * its operation, reading only the request fields named below from the
 * input buffer's first len bytes; it writes the JSON reply to the output
 * buffer and returns its length. */
#ifndef EXACT_WEB_H
#define EXACT_WEB_H

#include "exact.h"

#ifdef __cplusplus
extern "C" {
#endif

/* A web view's dialog, file input, download or print, held for its answer
 * ({"node":N,"capability":C,"args":A}, LLP 1020.002 §10). */
uint32_t exact_frame_request(ExactRuntime rt, size_t len);

#ifdef __cplusplus
}
#endif

#endif
