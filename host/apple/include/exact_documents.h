/* exact_documents.h — the documents family's agent operation (LLP 1047.001 D11): the agent operations this
 * family's Swift asks in a production build, each its own export
 * (`exact_apple::answers_exports!`). Each answers as `exact_agent` answers
 * its operation, reading only the request fields named below from the
 * input buffer's first len bytes; it writes the JSON reply to the output
 * buffer and returns its length. */
#ifndef EXACT_DOCUMENTS_H
#define EXACT_DOCUMENTS_H

#include "exact.h"

#ifdef __cplusplus
extern "C" {
#endif

/* A documents operation ({"op":…,"owner":O,…}, LLP 1069.010 D1). */
uint32_t exact_documents(ExactRuntime rt, size_t len);

#ifdef __cplusplus
}
#endif

#endif
