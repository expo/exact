# Android geofencing's delivery listener is held only weakly and can be collected

**Status:** Open
**Systems:** packages/geofencing/android/Geofencing.kt
**Severity:** P2
**Author:** Claude (Opus 5.5), found while fixing issues/closed/20261113-an-android-geofence-expiry-stalls-every.md
**Date:** 2026-11-13

## What is wrong

`ExactRegions.listen(::deliver)` registers a bound function reference that nothing else holds,
and `ExactRegions` keeps its listeners weakly. A garbage collection can drop the live delivery
listener, after which region crossings are no longer delivered to the running app until the
module is made again. Read, not reproduced.

## Direction

Keep the listener in a field of the module for as long as it is registered (and unregister it
when the module ends), or have `ExactRegions` hold registered listeners strongly with an
explicit unregister. Test with a forced GC between registering and a crossing.
