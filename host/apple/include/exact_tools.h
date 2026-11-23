/* exact_tools.h — the tools family's agent operation (LLP 1047.001 D11): the agent operations this
 * family's Swift asks in a production build, each its own export
 * (`exact_apple::answers_exports!`). Each answers as `exact_agent` answers
 * its operation, reading only the request fields named below from the
 * input buffer's first len bytes; it writes the JSON reply to the output
 * buffer and returns its length. */
#ifndef EXACT_TOOLS_H
#define EXACT_TOOLS_H

#include "exact.h"

#ifdef __cplusplus
extern "C" {
#endif

/* A system's call of a tool (LLP 1133.023 §10): {"op":"call",…}, and
 * {"op":"called","id":N} or {"op":"cancel","id":N} for the call it named.
 * {"op":"press","control":K,"digest":D,"cold":B,"hidden":B} is a person's
 * press of a compiled control (LLP 1126.004 D2), answered as a call is. */
uint32_t exact_tool(ExactRuntime rt, size_t len);

#ifdef __cplusplus
}
#endif

#endif
