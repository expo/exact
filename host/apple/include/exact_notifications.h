/* exact_notifications.h — the notifications family's agent reads (LLP 1047.001 D11): the agent operations this
 * family's Swift asks in a production build, each its own export
 * (`exact_apple::answers_exports!`). Each answers as `exact_agent` answers
 * its operation, reading only the request fields named below from the
 * input buffer's first len bytes; it writes the JSON reply to the output
 * buffer and returns its length. */
#ifndef EXACT_NOTIFICATIONS_H
#define EXACT_NOTIFICATIONS_H

#include "exact.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The background actions that ended since the last call, delivered
 * ({"ended":[…]}), and the notification categories the app declares. */
uint32_t exact_ended_actions(ExactRuntime rt, size_t len);
uint32_t exact_declared(ExactRuntime rt, size_t len);

#ifdef __cplusplus
}
#endif

#endif
