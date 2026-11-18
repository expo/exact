/* exact_traces.h — the traces family's agent reads (LLP 1047.001 D11): the agent operations this
 * family's Swift asks in a production build, each its own export
 * (`exact_apple::answers_exports!`). Each answers as `exact_agent` answers
 * its operation, reading only the request fields named below from the
 * input buffer's first len bytes; it writes the JSON reply to the output
 * buffer and returns its length. */
#ifndef EXACT_TRACES_H
#define EXACT_TRACES_H

#include "exact.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The work done by plan site ({"op":"perf"}) and the journal
 * ({"op":"logs","since":N}), which Save Trace keeps. */
uint32_t exact_perf(ExactRuntime rt, size_t len);
uint32_t exact_logs(ExactRuntime rt, size_t len);

#ifdef __cplusplus
}
#endif

#endif
