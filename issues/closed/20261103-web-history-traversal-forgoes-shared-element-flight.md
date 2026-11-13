# Web: a history traversal forgoes the shared-element flight

**Status:** Closed
**Resolution:** A web Back or Forward now flies its shared element: during the traversal the router mirrors its settled value to the history before the view transition's deferred tree update, which does not apply it again, so `popped` judges at once, and the agent's `travel` waits for the update; a traversal the browser animated (`hasUAVisualTransition`) or under reduced motion cuts.
**Systems:** web host history (`host/web/navigation.js`), JS target shared elements (`host/web-js/shared.js`, `host/web-js/rt.js`)
**Severity:** P3
**Author:** unrecorded (filed without a header)
**Date:** 2026-11-03
**Needs:** a decision that a history traversal runs its shared-element flight on the web, and how
the history mirror then judges it. LLP 1007 (the web host's history table) has `popped` judge a
traversal synchronously: the root's `navigate` applies its batch "before the mirror checks
acceptance". LLP 1013.000 D7 runs a paired flush inside `startViewTransition`, whose update the
browser calls later. Flying a traversal means amending LLP 1007 so that acceptance, the
`echo`/`drain` queue and the agent's `travel` wait for that update, and saying how a later commit
landing in the same update is told apart; LLP 1013.000 D7 would name traversals as flying.

**Found:** 2026-11-03, building an app whose product photo carries a `sharedElement` name on the
shop grid's card and on the product page (each route's content mounted only while shown in a browser,
so the pair leaves and arrives in one commit).

**What happened:** on the JS target, a commit whose flush may hand a name on runs its tree update
inside `document.startViewTransition` (`host/web-js/shared.js`), which the browser calls
asynchronously. The page's history (`host/web/navigation.js` `popped`) dispatches a Back or Forward
to the root's `navigate` handler and judges the result at once: with the update deferred, no router
change had landed, so it logged `Back to "/shop?c=tees" refused: … committed no router change;
restoring the entry` and put the product's entry back, while the deferred update then showed the shop.
The app and the address bar disagreed, and the agent's `back` reported the app kept the route.

**Now:** `navigation.traversing` is true while `popped` dispatches, and `rt.js`'s commit runs the
tree update at once then, forgoing the flight: a Back lands correctly, and only the forward
navigation (a link, a press) flies.

**Open:** a traversal could fly too if `popped` waited for the deferred update before judging it.
That makes `popped` asynchronous: the `echo`/`drain` queue and the agent's `travel` would wait on
the transition's update callback (shared.js `pending`), and `apply` would have to be told apart from
a later commit's. Not done here.
