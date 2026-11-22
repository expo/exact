/* exact_share.h — the share family's agent operation (LLP 1047.001 D11): the agent operations this
 * family's Swift asks in a production build, each its own export
 * (`exact_apple::answers_exports!`). Each answers as `exact_agent` answers
 * its operation, reading only the request fields named below from the
 * input buffer's first len bytes; it writes the JSON reply to the output
 * buffer and returns its length. */
#ifndef EXACT_SHARE_H
#define EXACT_SHARE_H

#include "exact.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Copy the app file a share names ({"path":P,"to":T}) for the sheet. With
 * {"op":"shareOpen","path":P}, open it as the copy would, for a drag, and
 * answer {"fd":N}: a descriptor the caller then owns and must close. */
uint32_t exact_share_file(ExactRuntime rt, size_t len);

#ifdef __cplusplus
}
#endif

#endif
