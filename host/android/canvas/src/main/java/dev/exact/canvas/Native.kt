package dev.exact.canvas

import android.graphics.Bitmap
import java.nio.ByteBuffer

/** exact2 on the main thread: layout, shaping and the paint walk, recorded as Canvas ops. */
object Native {
    init {
        // mimalloc reads its options as the library loads, whichever entrant loads it (an activity, a job, a
        // receiver). Freed memory goes back at once (decoded pictures otherwise stay held), unless a launch said
        // otherwise first.
        android.system.Os.setenv("MIMALLOC_PURGE_DELAY", "0", false)
        System.loadLibrary("exactcanvas")
    }
    /** Copy the launch environment into the library's libc before any host thread boots. ARM translation can
     * give Android's Os and the library separate environments. */
    @JvmStatic external fun environment(values: ByteArray)
    /** Bind the application JVM before any host starts TLS work. The host keeps [context] for the process and gives
     * it to a GPU artifact's initialization entry (LLP 1009 D11), so it is the application's, never an activity's. */
    @JvmStatic external fun trust(context: android.content.Context)
    /** Boot the app's host on the calling thread, in a window of this size in pixels ([window] told its insets);
     * whether it booted. Where a host already runs on this thread (the activity was made again in this process,
     * LLP 1115.001 D15.1) nothing boots: that host is adopted in this window, and paints everything again for the
     * reader that calls, which has none of the last one's rows, faces or pictures. `dir` is the app's files
     * directory; `store` its no-backup directory, where a library with an update store keeps it (LLP 1030.006 D3). */
    @JvmStatic external fun start(width: Int, height: Int, scale: Float, dir: String, store: String, live: Boolean): Boolean
    /** [start] on a thread of its own; [adopt] takes the host. Where a host already runs in this process nothing
     * boots, and [adopt] adopts that one as [start] does. */
    @JvmStatic external fun startAsync(width: Int, height: Int, scale: Float, dir: String, store: String, live: Boolean)
    /** Wait for [startAsync]'s host and run it on this thread from now on; whether it booted. */
    @JvmStatic external fun adopt(): Boolean
    /** The device's settings, whole ([Device], LLP 1115.001 D3.1): before [start], from any thread, so the host boots
     * with them, and again on the host's thread whenever one changes. */
    @JvmStatic external fun configure(
        night: Boolean, fontScale: Float, locale: String, rtl: Boolean, hour24: Boolean, noAnimations: Boolean,
        boldText: Boolean, highContrast: Boolean, touch: Boolean, tv: Boolean, touchSlop: Float, dates: String,
        formatLocale: String, screenReader: Int,
    )
    /** The device's colours ([Palette], LLP 1115.001 D4): Material 3's colour roles as the reader resolved them for
     * this device, twelve under the light scheme and then the same twelve under the dark one, ARGB. Before [start],
     * from any thread, so the host boots resolving every colour role by them, and again on the host's thread whenever
     * one changes. */
    @JvmStatic external fun colors(roles: IntArray)
    /** The window (LLP 1115.001 D3.3): its size in pixels, the display's pixels a point, and the system's raw insets
     * in pixels, sixteen numbers: top, right, bottom, left for the system bars, then the display cutout, the software
     * keyboard and the system's gestures. Before [start], from any thread, so the host boots laid out in it, and on
     * the host's thread again whenever any of it changes. The reader pads nothing: [chrome] says where the page is. */
    @JvmStatic external fun window(width: Int, height: Int, scale: Float, insets: IntArray)
    /** What the reader draws around the page, when that is not what this last returned (else null): where the page is
     * in the window (x, y, width, height, in pixels), the colour behind it and in the edges it keeps out of (ARGB),
     * and 1 when its scheme is dark (the bars' icons are then light). */
    @JvmStatic external fun chrome(): IntArray?
    /** Publish pending work. A Choreographer turn also delivers frame tasks. */
    @JvmStatic external fun frame(vsync: Boolean): ByteBuffer?
    @JvmStatic external fun poll(): Boolean
    /** Advance tasks without recording a frame; milliseconds until the next task, or -1 while idle. */
    @JvmStatic external fun background(applying: Boolean): Double
    @JvmStatic external fun acquired(running: Boolean)
    @JvmStatic external fun timerObservation(scrolling: Boolean, sampling: Boolean, applying: Boolean, idle: Boolean): Boolean
    @JvmStatic external fun timerHeld(): Int
    /** A display tick: Choreographer's frame time, when its callback began (`System.nanoTime`), the display's fastest
     * period in nanoseconds, thermal code, and this tick's scheduled period ([HostFacts.turned]). */
    @JvmStatic external fun displayFrame(vsync: Long, arrival: Long, fastest: Long, thermal: Int, period: Long)
    /** `perf frames` (LLP 1079.000 §6.2), all on exact2's thread and in `System.nanoTime`: a Choreographer turn at
     * [vsync], its frame's [deadline] and its vsyncs' [period] (0 where Choreographer gave none), when it ran from
     * [arrival] to [finished], and the publication it made (0: none), whose ingest finishes its work. */
    @JvmStatic external fun frameTurn(arrival: Long, vsync: Long, deadline: Long, period: Long, finished: Long, publication: Long)
    /** The main thread ingested every publication through [applied], from [began] to [ended]. */
    @JvmStatic external fun frameLanded(applied: Long, began: Long, ended: Long)
    /** No turn is asked for: the run of turns ends. */
    @JvmStatic external fun frameIdle()
    /** The display's refresh period: a GPU canvas's frame period (LLP 1009) and a turn's where Choreographer gives
     * none. */
    @JvmStatic external fun framePeriod(period: Long)
    /** [count] window frames ([WindowFrames], five words each) and the reports [lost] before they were read. */
    @JvmStatic external fun frameWindow(words: LongArray, count: Int, lost: Long)
    /** The device a `perf frames report` names, as JSON. */
    @JvmStatic external fun frameIdentity(identity: ByteArray)
    /** The frame rates GPU canvases ask for (`Surface::max_fps`) that changed since last asked: view and rate (0: the
     * display's) pairs, or null. */
    @JvmStatic external fun frameRates(): IntArray?
    /** Whether a turn is needed. With [vsyncOnly], whether it needs a Choreographer callback. */
    @JvmStatic external fun animating(vsyncOnly: Boolean = false): Boolean
    @JvmStatic external fun owed(): Boolean
    /** After the reader's [scrolled]: whether a [frame] now would only move grouped rows the reader moved itself, with
     * nothing to tell it but those moves. The move is then taken, and there is no frame. */
    @JvmStatic external fun moved(): Boolean
    /** The collection pass a scroll left; `limit` >= 0 slices it (rows past what shows per list), the scrolled list
     * leading toward `velocity` px/s. Bit 0: a paint is wanted; bit 1: rows are left for another slice. */
    @JvmStatic external fun refine(limit: Int, velocity: Float): Int
    @JvmStatic external fun nextDue(): Double
    /** The token of the picture the last [frame] painted, to report with its present ([Presents]); 0 when exact2 asks
     * for none. */
    @JvmStatic external fun presentToken(): Long
    /** The picture [token] reached the screen at [at] (`System.nanoTime`): whether a turn is now wanted. */
    @JvmStatic external fun picturePresented(token: Long, at: Long): Boolean
    /** The wake descriptors, -1 where absent: executor replies, decoded images, the GPU module's acquired images
     * ([Wakes.ACQUIRED]) and the update store's finished checks. */
    @JvmStatic external fun fds(): IntArray
    /** A driver from outside scrolls the feed by [dy] px ([ExactActivity.drive]). A finger's scroll is the
     * calls below. */
    @JvmStatic external fun scroll(dy: Float)
    /** What is under a point of the view (pixels), for the reader to decide what a drag from there is (LLP 1115.001
     * D7.3) and whose a touch there is (D8): the axes exact2's own gestures there claim (bit 0 sideways, bit 1 up
     * and down; bit 2: the latter is a sheet's drag alone, which a hosted view that disallows interception keeps,
     * [Contacts]; bits 3 and 4: a nested sheet takes an upward or downward finger drag), a count, the hosted text field the touch is for (0: none; [Fields]), and eleven numbers a scroller,
     * innermost first: its id, flags (bit 0 it takes a sideways drag from here, bit 1 one up or down, bit 2 its
     * rows are a node the reader can move, bit 3 no scrollbar, bit 4 it hears `refresh`, bit 5 its `refreshing`
     * is true, bits 6 and 7 its `keyboardDismissMode`), its revision, then as float bits its offset (x, y), its farthest offset
     * (x, y) and the box it shows in (x, y, width, height). exact2 tells the reader of each of them from then on
     * ([scrollers]). After the scrollers: the native module view under the point (0: none), then its painted
     * border box x, y, width and height as float bits, in pixels, its allocation slot and generation, how many long
     * presses a node has taken so far (wrapping), the session token's low and high words, its painted content origin
     * x and y as float bits, and the node a long press there gives to, a context menu's or a drag's (0: none). Then
     * each scroller's snap, in the same
     * order (`host/painted/src/presenter/snap.rs`,
     * `reader_snaps`): flags (bit 0 it snaps sideways, bit 1 up and down, bit 2 `mandatory`), then sideways and then up
     * and down its snapport's length as float bits, a count, and per snap area the least and greatest offsets it rests
     * at as float bits and 1 for `scroll-snap-stop: always`, in pixels; flag bit 3: a virtualized list, whose areas past
     * these [snapsAround] tells. */
    @JvmStatic external fun readerSelection(action: Int, x: Float, y: Float): String?
    @JvmStatic external fun under(x: Float, y: Float): IntArray
    /** Virtualized list [id]'s snap areas up and down ([vertical]) or sideways, for a release at offset [release] whose
     * rest is projected near [projected] (pixels): a count, then per area its least and greatest offsets as float bits
     * and 1 for `scroll-snap-stop: always` (`host/painted/src/presenter/snap.rs`, `reader_snaps_around`). */
    @JvmStatic external fun snapsAround(id: Int, vertical: Boolean, release: Float, projected: Float): IntArray
    /** What exact2 tells the reader of the text fields it hosts (LLP 1115.001 D8), when that is not what this last
     * returned (else null): a count, then for each field, in tree order: its id, its generation (moved on when its
     * node became another row's item: the view starts over), flags (bit 0 it was painted and has a place, bit 1 it
     * takes no touch and no focus) and the scroller whose rows it was painted among (0: none); as float bits its
     * content box and the clip it shows through (x, y, width, height each; a negative clip width: none); the count
     * of the reader's edits that reached exact2 and the value the app shows as of it (a string: its bytes' count,
     * then UTF-8 padded to whole words); its look, eighteen words (`Presenter::reader_fields` lists them), and five
     * strings: its placeholder, its `autocomplete`, its font's file, its axes and its OpenType features. A field it
     * leaves out is gone. */
    @JvmStatic external fun fields(): IntArray?
    /** Hosted field [id], known at [generation], took the focus (a finger tapped it) or lost it. On the host's
     * thread, as every `field` call is; what one says of a generation exact2 has moved on from is dropped. */
    @JvmStatic external fun fieldFocused(id: Int, generation: Int, focused: Boolean)
    /** Hosted field [id], known at [generation], had real input that may change nothing exact2 hears of: a finger
     * down in it, a key down. [kind] is 0 for IME input, 1 for a pointer, 2 for a hardware key that shows focus.
     * A state held on it (LLP 1122 D8) lets go. */
    @JvmStatic external fun fieldInput(id: Int, generation: Int, kind: Int)
    /** Hosted field [id] holds [value] after the person's edit number [seq], its selection at [start] to [end]
     * (UTF-16 units). Composing text is part of the value: exact2 fires `input` as the text composes. */
    @JvmStatic external fun fieldEdited(id: Int, generation: Int, seq: Int, value: String, start: Int, end: Int)
    /** Hosted field [id] began to compose, holding [value], or its composition ended, committed or cancelled: told
     * before the edit that began or ended it. While it composes exact2 holds a value the app writes and tells the
     * field its own text; once it ends, the value the app then shows (LLP 1115.000 §5, ruling 6). */
    @JvmStatic external fun fieldComposing(id: Int, generation: Int, composing: Boolean, value: String)
    /** Hosted field [id] moved its selection with no edit. */
    @JvmStatic external fun fieldScrolled(id: Int, generation: Int, x: Int, y: Int)
    @JvmStatic external fun fieldSelected(id: Int, generation: Int, start: Int, end: Int)
    /** Hosted field [id] had its action key pressed, or Enter or Tab on a hardware keyboard. [action] is what the
     * key said it does: 0 Enter with no action, 2 done, 3 go, 4 next, 5 previous, 6 search, 7 send, 16 Tab, 17
     * Shift+Tab. exact2 does that: Enter's `key` handlers, `change` and `submit`, then the move or the leave. */
    @JvmStatic external fun fieldAction(id: Int, generation: Int, action: Int)
    /** A module event, checked against the session and allocation carried by its painted view. */
    @JvmStatic external fun moduleEvent(id: Int, slot: Int, generation: Int, session: Long, kind: String, payload: String)
    @JvmStatic external fun frameLoaded(id: Int, slot: Int, generation: Int, session: Long)
    /** An iframe's view took the window's focus (a field in its page did) or lost it; checked as [frameLoaded] is. */
    @JvmStatic external fun frameFocused(id: Int, slot: Int, generation: Int, session: Long, focused: Boolean)
    /** An iframe's `FrameNavigation` record (LLP 1020.002 D2): [event] by its name, [record] its JSON; checked as
     * [frameLoaded] is. */
    @JvmStatic external fun frameEvent(id: Int, slot: Int, generation: Int, session: Long, event: String, record: String)
    /** A media element's event ([event] by HTML's name, [payload] its value), or with an empty [event] its observed
     * [state] for `state.media` (LLP 1042.001 D7); checked as a module event is. */
    @JvmStatic external fun media(id: Int, slot: Int, generation: Int, session: Long, event: String, payload: String, state: String)
    @JvmStatic external fun backgroundAudio(id: Int, slot: Int, generation: Int, session: Long): ByteArray?
    /** The reader moved scroller [id] by ([dx], [dy]) px, knowing revision [rev] of it. exact2 clamps the move to
     * the scroller's travel, and drops one sent under a revision the app's own scroll has moved on from. */
    @JvmStatic external fun scrolled(id: Int, dx: Float, dy: Float, rev: Int)
    /** A finger began dragging scroller [id] (its `scrolldragstart`), or ([ended]) let go of it (its
     * `scrolldragend`): at ([vx], [vy]) px/s, positive as its offset grows, and, with [rest], to come to rest at
     * ([restX], [restY]) px; [flings] when it threw a fling. From the start until the lift, or the fling's end
     * ([flingEnded]), the scroller's moves owe no `scrollend`. */
    @JvmStatic external fun sheetDrag(index: Int, generation: Int, top: Float, ended: Boolean, velocity: Float)
    @JvmStatic external fun scrollDrag(id: Int, ended: Boolean, vx: Float, vy: Float, rest: Boolean, restX: Float, restY: Float, flings: Boolean)
    /** A pull to refresh of scroller [id] was let go of past its threshold: its `refresh` ([Refresh]). */
    @JvmStatic external fun refresh(id: Int)
    /** The fling a drag of scroller [id] threw stopped: at rest, caught by a finger, or ended by the app's scroll. */
    @JvmStatic external fun flingEnded(id: Int)
    /** A pinch of zoomable scroller [id] (LLP 1057.001.000 §5; [Zooming]): [phase] 0 it began, 1 it scaled by
     * [factor] while its focal point moved from ([x0], [y0]) to ([x1], [y1]) px, 2 it ended. */
    @JvmStatic external fun zoom(id: Int, phase: Int, factor: Float, x0: Float, y0: Float, x1: Float, y1: Float)
    /** A double tap at ([x], [y]) px: [phase] 0 its second tap came down (the first tap's press, held for it, never
     * comes), 1 it lifted, which zooms the scroller under it (D5). */
    @JvmStatic external fun zoomTap(phase: Int, x: Float, y: Float)
    /** A wheel turned [dy] points toward the end with `ctrl` held at ([x], [y]) px: a zoom about the pointer. */
    @JvmStatic external fun zoomWheel(x: Float, y: Float, dy: Float)
    /** What exact2 tells the reader of the scrollers it knows, when that is not what this last returned (else
     * null). The producer's scale as float bits, then nine words each: id, flags, revision, and pairs of float bits
     * for its current offset in points, recorded offset in pixels, and logical maximum in points. The revision
     * moves on when the app scrolled it (a `scrollTop` write, a reveal): the reader then takes this offset and ends
     * its fling. A scroller the reader knew that is not among them is gone. */
    @JvmStatic external fun scrollers(): IntArray?
    /** A touch (0 down, 1 up, 2 move, 3 cancel) at the view's pixels, and what touched: the event's tool type
     * ([android.view.MotionEvent.getToolType]). A finger is reported to the app as a touch that hovers nothing and
     * leaves no focus ring, a mouse as a mouse (LLP 1115.001 D7.4). Action 4 is no new touch: the one that is down
     * has been held still for the system's long-press time ([android.view.ViewConfiguration.getLongPressTimeout]).
     * A node with `draggable` nearer than any menu asks for its drag (request kind 23, LLP 1094.000 D7); else what
     * names a context menu there opens it (D10), and a move past the touch slop then lifts the node's drag. That touch
     * presses nothing when it lifts. */
    @JvmStatic external fun touch(action: Int, x: Float, y: Float, tool: Int)
    /** A finger past the touch's first ([Fingers]): 0 down, 1 up, 2 move, 3 cancel, at the view's pixels, by its pointer
     * [id] ([android.view.MotionEvent.getPointerId]). The canvas holding the touch takes it where it lands on that
     * canvas; where no canvas holds the touch, a second finger cancels it. */
    @JvmStatic external fun finger(action: Int, id: Int, x: Float, y: Float)
    /** Whether a finger, a pen or a mouse button is down on the view: true at the touch's down, before the reader
     * routes it, and false at its last lift or the system's cancel. A scroll, a field or a hosted view takes a touch
     * that is still down, and exact2 then hears a cancel and no up ([touch]); a replacement waits for the finger
     * itself (LLP 1029.000.002 D9). Whether one was waiting for this lift, and so wants a turn. */
    @JvmStatic external fun touching(on: Boolean): Boolean
    /** What happened in the window the reader made for surface [serial] of the top layer ([Surfaces], LLP 1115.001
     * D10), on the host's thread: a touch ([what] 0 down, 1 up, 2 move, 3 cancel) at ([x], [y]), pixels from the
     * window's own top left; the system dismissed the window (8: a tap outside a menu, Back, Escape); the reader
     * could not make it (9); a key (16 Enter, 17 Tab, 18 Shift+Tab, 19 to 22 the arrows up, down, left and right, 23
     * Space). What is said of a surface that is no longer the one open is dropped. */
    @JvmStatic external fun surface(serial: Int, what: Int, x: Float, y: Float)
    /** What exact2 tells the reader of its bars ([Bars], LLP 1115.001 D11), when that is not what this last returned
     * (else null): a route's header as Material's top app bar and the navigation root's tablist as its navigation
     * bar, each painted by exact2 and shown over the page. A count, then for each bar which it is (1 the top app
     * bar, 2 the navigation bar), its frame in the window (x, y, width, height, pixels), a count of words and that
     * many: its picture, an op stream of its own, whole. A bar a statement leaves out is gone. Taken with each
     * stream, so a bar and the page under it change in one frame. */
    @JvmStatic external fun bars(): IntArray?
    /** What happened in bar [which] (1 the top app bar, 2 the navigation bar), on the host's thread: a touch ([what]
     * 0 down, 1 up, 2 move, 3 cancel) at ([x], [y]), pixels from the bar's own top left. exact2 knows what is
     * there: the Up arrow goes Back, a header's button and a destination are pressed. */
    @JvmStatic external fun bar(which: Int, what: Int, x: Float, y: Float)
    /** What exact2 tells the reader of the accessibility tree ([Access], LLP 1115.001 D13), when that is not what
     * this last returned (else null). Called only while an accessibility service is on; exact2 builds nothing
     * otherwise. [again]: the reader holds nothing of the tree (it was made again, or a service was just turned on)
     * and is told it whole. The words: the statement's number; the surface whose window the nodes are in (its
     * serial; 0: the page's window); 1 when the tree is told whole (the only changes then listed speak); the
     * identity of the route that shows (0: none) and its title, a string (its bytes' count, then UTF-8 padded to
     * whole words); a count of changes and four words each (a node's identity, 0 for the view itself; what changed:
     * bit 0 its subtree, bit 1 its text, bit 2 its state, bit 3 it scrolled, bit 4 it is a live region that
     * speaks; and as float bits how far a scroller moved, x and y, in pixels); a count of nodes and for each, in
     * the order a person reads: its identity, its parent's (0: the view), its view, its role and its flags
     * ([Access]'s constants), its box (x, y, width, height, pixels: in the window, or in its own window for a
     * surface), as float bits its range's least, greatest and present value (a scroller's offset and its farthest),
     * its place in its set from 1 and the set's size, and three strings: its name, its value and its description. */
    @JvmStatic external fun access(again: Boolean): IntArray?
    /** An accessibility service acts on node [id] of the tree last told, on the host's thread: [action] 1 press
     * (the presenter's own press at the node, as a driver's `tap`), 2 hold (its context menu), 3 scroll a scroller
     * on or step a range up, 4 back or down, 5 set a range to [value], 6 start the node's drag out of the app. Whether
     * anything took it. */
    @JvmStatic external fun accessAct(id: Int, action: Int, value: Float): Boolean
    /** No accessibility service is on any more: exact2 forgets the tree it told and builds none until [access]. */
    @JvmStatic external fun accessOff()
    /** One slice ([budgetUs] of shaping) of laying out what the touch that is down would open; whether more is left. */
    @JvmStatic external fun ahead(budgetUs: Int): Boolean
    @JvmStatic external fun copyImage(id: Int, bitmap: Bitmap): Boolean
    /** Picture [id] into a GPU buffer (one copy; no upload at first draw). */
    @JvmStatic external fun copyImageToBuffer(id: Int, buffer: android.hardware.HardwareBuffer): Boolean
    /** A GPU canvas's window (op 28 kind 3): the ANativeWindow, to pass to [detachSurface], or 0. */
    @JvmStatic external fun attachSurface(view: Int, surface: android.view.Surface, width: Int, height: Int): Long
    @JvmStatic external fun detachSurface(view: Int, window: Long)
    /** A GPU canvas without a window: it renders into buffers this reader draws ([canvasFrame], [canvasBuffer]). */
    @JvmStatic external fun attachBuffers(view: Int, width: Int, height: Int)
    /** A buffered canvas's newest frame (any thread): generation (24 bits), slot (8), serial (32); -1 none yet, -2 no
     * buffers on this device. */
    @JvmStatic external fun canvasFrame(view: Int): Long
    /** -1 attachment refused, 0 waiting, 1 handed to the compositor, 2 submitted, 3 explicitly empty.
     * Any thread; an empty canvas shows its own styles. */
    @JvmStatic external fun canvasReady(view: Int): Int
    /** That frame's buffer, or null. */
    @JvmStatic external fun canvasBuffer(view: Int): android.hardware.HardwareBuffer?
    /** A key at the focused node when no hosted field holds the focus: a character (or '\n' for Enter), or
     * Backspace. A hosted field ([Fields]) takes its keys itself. */
    @JvmStatic external fun key(code: Int, backspace: Boolean)
    /** A gamepad's state for the canvas a key would reach (LLP 1127.003 D2): [values] its 17 standard buttons, then
     * its 4 axes; [connected] false when it went away. [at] is the event's `eventTime`
     * (`SystemClock.uptimeMillis()`), 0 for a send no event caused: the host stamps it on arrival then. */
    @JvmStatic external fun pad(index: Int, connected: Boolean, values: FloatArray, at: Long)
    /** A key, down or up, by the web's `code` and `key` ([Keys.named]): Tab, or a remote's D-pad arrow or Select.
     * [shift]: Shift is held with it, which Android tells with the key rather than as a key of its own. */
    @JvmStatic external fun namedKey(code: String, key: String, down: Boolean, repeat: Boolean, shift: Boolean)
    /** System Back (LLP 1115.001 D6): whether the app took it. From its navigation root's selected route it goes to
     * the Back control the root names, else to the root's `navigate` handler, else it is the runner's own `back`;
     * false when there is nowhere to go back to, and the activity finishes. The reader takes Back only while the
     * host says it has somewhere to go (a request, [requests]). */
    @JvmStatic external fun back(): Boolean
    /** Predictive Back as it goes (Android 14): `phase` 0 started, 1 moved, 2 cancelled; `progress` 0 to 1 as the
     * system eases it; `edge` 0 the left, 1 the right. Whether exact2 previews it. The commit is [back]. */
    @JvmStatic external fun backProgress(phase: Int, progress: Float, edge: Int): Boolean
    /** A URL given to the running app: its location goes to the navigation root's `navigate` (LLP 1133.023 D15 item 3). */
    @JvmStatic external fun open(href: String)
    /** A share given to the running app, `{"target", "share"}` as [Shares.request] writes it (LLP 1133.024 D5). */
    @JvmStatic external fun share(request: String)
    /** The URL in the launching intent of the first activity made over a host [Headless] booted (LLP 1133.017 D7): its
     * location is the launch location, as `EXACT_LAUNCH_URL` is at a boot, with no `navigate`. */
    @JvmStatic external fun launch(href: String)
    /** The share in that intent, `{"target", "share"}`: claimed as `EXACT_SHARE` is at a boot. */
    @JvmStatic external fun launchShare(request: String)
    /** What exact2 asks of the platform since this last returned any (LLP 1115.001 D5), or null when it asks
     * nothing: the one channel out, which [Requests] reads and acts on, on the main thread. exact2 queues a request
     * and makes the first of [fds] readable, so the reader comes for it whether or not a frame follows. The words:
     * for each request its kind, its id, the count of words that follow, and those words. */
    @JvmStatic external fun requests(): IntArray?
    /** The answer to request [id] (the share sheet's: `shared` or `dismissed`, or why it could not be shown), or null
     * when it was cancelled. On the host's thread. */
    @JvmStatic external fun answer(id: Int, answer: String?)
    /** A topic an extension announced ([ExtensionContext.changed]): its readers are asked again. On the host's thread. */
    @JvmStatic external fun changed(topic: String)
    /** A person's act on an extension's own platform UI ([Activations]): its event [event] with [args], a JSON array,
     * raised with a person's origin (LLP 1133.004 D6); for a notification's action, only while the plan declares
     * [action], the action's record. On the host's thread. */
    @JvmStatic external fun activate(extension: String, event: String, args: String, action: String?)
    /** A person's press of control [kind], installed with [digest] ([Controls], LLP 1126.004 D2): `{"call": id}`, or
     * `{"error"}` for one the runner refuses outright. With [foreground] it is pressed from the app in front, which a
     * tile opened for it. On the host's thread. */
    @JvmStatic external fun press(kind: String, digest: String, foreground: Boolean, cold: Boolean, hidden: Boolean): String
    /** Press [id] ends now: superseded, its app left the front, or the phone locked. On the host's thread. */
    @JvmStatic external fun cancelPress(id: Long)
    /** Whether press [id] still runs uncancelled within its bound. On the host's thread. */
    @JvmStatic external fun pressLive(id: Long): Boolean
    /** A line for the session's journal. On the host's thread. */
    @JvmStatic external fun journal(line: String)
    /** The extensions the APK carries, one a line ([Extensions]); under the agent, told so unless the drive names them
     * (`--bind`). On the host's thread. */
    @JvmStatic external fun bindExtensions(names: String)
    /** How a presence check ended ([Presence]), on the store's completion path: never journaled. On the host's thread. */
    @JvmStatic external fun presence(answer: String)
    /** A live activity send's answer by its ticket, or the activities the shade shows ([Activities], LLP 1126 D13).
     * On the host's thread. */
    @JvmStatic external fun activities(report: String)
    /** A package's picture for request [id] (LLP 1133.019 D11 item 1): [bitmap], software `ARGB_8888`, or why there is
     * none ([error]) when it is null. On the host's thread. */
    @JvmStatic external fun picture(id: Int, bitmap: android.graphics.Bitmap?, error: String?)
    /** Picture [source] changed, so it is asked for again, or was [revoked]: every box showing it is emptied. On the
     * host's thread. */
    @JvmStatic external fun pictureChanged(source: String, revoked: Boolean)
    /** Activity visibility and the process's execution eligibility. On the host's thread. */
    @JvmStatic external fun hidden(hidden: Boolean, eligible: Boolean)
    /** The reader drew a frame of the running generation, which makes its boot good (LLP 1030.006 D5); whether to say
     * so again after the next. On the exact thread. */
    @JvmStatic external fun drew(): Boolean
    /** Whether the host's data module is active: 1, 0 while its image loads, -1 when it failed to activate. With
     * [unshown], a host no reader takes a frame from activates it now, since no pixel waits for it (LLP 1133.017 D7);
     * its image may still be loading then, and the host's wake says when it is not. On the host's thread. */
    @JvmStatic external fun dataActive(unshown: Boolean): Int
    /** Waits up to [ms] for the store's writes, which exact2 makes on a thread of its own: whether every commit handed
     * over was written (a write that failed is tried once more; failing again, it stays pending). Any thread. */
    @JvmStatic external fun storeDrain(ms: Int): Boolean
    /** What [Orientation] asked of the activity for the session's orientation lock (LLP 1133.029 §8), JSON, whether
     * its resolution found a conflict, and a line for the journal or null. On the host's thread. */
    @JvmStatic external fun orientationApplied(applied: String, conflict: Boolean, line: String?)
    /** The frame clock, ms, retaining its monotonic origin across host turns ([ExtensionContext.now]). Under the
     * agent, held virtual time; during a capture lease, the display's live time. Any thread. */
    @JvmStatic external fun now(): Double

    /** The file an `app:/data|cache|tmp/…` path names, or null (`ExtensionContext.file`). */
    @JvmStatic external fun appFile(path: String): String?
    /** The regular file an `app:/data|cache|tmp/…` path names, opened for reading as the app's own `fs.read` opens it,
     * below its root and through no symbolic link ([Files]); null where it names none, and in a process whose reader
     * has not booted, which knows no `app:/` root. */
    @JvmStatic fun openApp(path: String): android.os.ParcelFileDescriptor? =
        appDescriptor(path).takeIf { it >= 0 }?.let(android.os.ParcelFileDescriptor::adoptFd)
    /** [openApp]'s descriptor for the caller to adopt, or -1. */
    @JvmStatic external fun appDescriptor(path: String): Int
    /** The reader's regular file [source] renamed to the `app:/` path [path], its missing directories made, each
     * directory reached through no symbolic link and a link found at [path] refused: whether [source] is now the file
     * at [path]. [source] is on the same file system. */
    @JvmStatic external fun publishApp(path: String, source: String): Boolean
    /** The running generation's asset directory when a signed update's is running, else null ([AssetRoot]). */
    @JvmStatic external fun assetRoot(): String?
    /** The app's directory [index] names: 0 `data`, 1 `cache`, 2 `tmp` ([ExtensionContext.data]). */
    @JvmStatic external fun appRoot(index: Int): String?
    /** Whether extension [name] is told it runs under the agent ([ExtensionContext.agent]). Any thread. */
    @JvmStatic external fun underAgent(name: String): Boolean
    /** Whether the session's storage is a drive's and not the app's own ([ExtensionContext.scratch]). Any thread. */
    @JvmStatic external fun scratchStorage(): Boolean
    /** A package's call of the app's handler, `{"extension", "handle", "handler", "args"}` (LLP 1133.016 D15 item 1):
     *  its number, or -1 with no session; [invoked] reads its answer. */
    @JvmStatic external fun invoke(body: String): Int
    /** Call [id]'s answer as JSON, once; null while it has none. */
    @JvmStatic external fun invoked(id: Int): String?
    /** Whether the page places a canvas of surface [name], as of the host's last turn ([ExtensionSurfaces]). Any
     * thread. */
    @JvmStatic external fun surfaceHas(name: String): Boolean
    /** A package's sample for surface [surface]'s canvas input (LLP 1133.013 D6), or with [ended] its end: kept until
     * the host's next display frame delivers it to the canvas. [values] x, y, z and w; [at] the session clock's ms.
     * Any thread; it waits for nothing. Whether none waited before it: the host is then woken for a frame. */
    @JvmStatic external fun sensorInput(surface: String, sensor: String, values: DoubleArray, dropped: Int, at: Double, ended: Boolean): Boolean
    /** What the app's Kotlin hatches ask, for node [node] or 0 for their module (LLP 1115.001.000 D4; the kinds are
     * `hatches::reader::act`'s): its answer, a ticket for a registration, -1 when refused. */
    @JvmStatic external fun hatchAct(node: Int, kind: Int, text: String?): Long
    /** One request of the agent API (LLP 1012), a JSON line as UTF-8, and its answer the same way: the Linux
     * host's handler (`host/painted/src/canvas/agent.rs`), on the host's thread. A seekable drive owns the clock
     * except during a frame capture. A `--live` drive uses the display clock from boot. [AgentDrive] serves adb. */
    @JvmStatic external fun agent(line: ByteArray): ByteArray
    /** What an [ExactForma] asks (LLP 1121 D12), on the host's thread: `{"view": id or 0, "names": [..]}`, answered
     * `{"inputs": "<u64>", "colors": {name: ARGB or null}}`, each colour as exact2 paints it at that view (its pinned
     * modes, its scheme), null for a name the design lacks; `inputs` moves when anything the answer read does. A
     * view that is no live node, or a name that is no string, is answered `{"error": ...}`. */
    @JvmStatic external fun forma(request: String): String
    /** Run the calling thread on the cores outside the slowest cluster; whether it was set. */
    @JvmStatic external fun fastCores(): Boolean
    /** An ADPF hint session over [tids] (this process's threads), [targetNs] their work's target. */
    @JvmStatic external fun hintSession(tids: IntArray, targetNs: Long): Boolean
    /** Input arrived: the [hintSession]'s threads clocked up for it; whether a hint was sent. */
    @JvmStatic external fun inputHint(): Boolean
    /** Run thread [tid] on the calling thread's core until it calls [fastCores]; whether it was set. */
    @JvmStatic external fun colocate(tid: Int): Boolean
    /** Thread [tid] on the fast cores only ([fast]), or anywhere again. */
    @JvmStatic external fun fastThread(tid: Int, fast: Boolean): Boolean
    /** Picture [id]'s own GPU buffer (decoded into it), or null when its pixels are on the heap. */
    @JvmStatic external fun imageBuffer(id: Int): android.hardware.HardwareBuffer?
    /** Decode pictures straight into GPU buffers ([imageBuffer]) from now on. */
    @JvmStatic external fun hardwarePictures(on: Boolean)
    /** Pictures announced and not fetched yet: id, width, height triples. */
    @JvmStatic external fun pendingPictures(): IntArray
    /** The feed: the scroller a finger last moved up or down, or a driver's [scroll] found (its group's id in the
     * stream); 0 before either. */
    @JvmStatic external fun feed(): Int
}
