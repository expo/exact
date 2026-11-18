/* exact_pickers.h — the file and photo pickers' agent operations (LLP 1047.001 D11): the agent operations this
 * family's Swift asks in a production build, each its own export
 * (`exact_apple::answers_exports!`). Each answers as `exact_agent` answers
 * its operation, reading only the request fields named below from the
 * input buffer's first len bytes; it writes the JSON reply to the output
 * buffer and returns its length. */
#ifndef EXACT_PICKERS_H
#define EXACT_PICKERS_H

#include "exact.h"

#ifdef __cplusplus
extern "C" {
#endif

/* What a file input's showPicker names ({"id":I}); hold its picker for an
 * answer ({"id":I} or {"view":V}, the reply naming the ticket); and where a
 * picked file named {"name":N} is copied in (LLP 1069.002). */
uint32_t exact_picker(ExactRuntime rt, size_t len);
uint32_t exact_show_picker(ExactRuntime rt, size_t len);
uint32_t exact_picked_path(ExactRuntime rt, size_t len);

#ifdef __cplusplus
}
#endif

#endif
