// chatprobe — inserted into a chat app (DYLD_INSERT_LIBRARIES) to measure launch and a scroll replay the way a
// person sees them: whether each bubble on screen has its text, photo or waveform drawn (README.md, "Metrics").
// It finds the transcript (the tallest UIScrollView), renders the window's layer tree at half a pixel per point
// on display-link frames and classifies pixels by WhatsApp's colours (README.md). It never scrolls: the replay's
// touches come from the XCTest runner (replay.swift), through UIKit's own gesture recognizers.
//
// Phases: `launch` from process start until the first touch; `up` from the first touch until message 1 is on
// screen; `rest` at the top until the next touch; `flings` from then until the runner posts
// `dev.exact.chatreplay.done`. When the list has sat at its visual top for 500 ms with its content size unchanged,
// and again each second it stays there, the probe posts `dev.exact.chatreplay.edge`; the runner checks that
// message 1 is there (a list that pages older rows in has a top at every page) and posts `dev.exact.chatreplay.top`,
// and the time to message 1 is the time to that edge. The probe answers `dev.exact.chatreplay.topok`, or
// `dev.exact.chatreplay.notop` for an edge the list has left since.
//
// Environment: CHAT_OUT (result path; default Documents/chatprobe.json), CHAT_SAMPLE (classify every Nth frame
// while scrolling; 0 = never, for pacing; default 2), CHAT_DUMP (a directory: PNGs of up to 40 frames with a
// missing bubble, and the first with a drawn bubble and nothing missing).
#import <UIKit/UIKit.h>
#import <QuartzCore/QuartzCore.h>
#import <notify.h>
#import <sys/sysctl.h>
#import <sys/resource.h>
#import <mach/mach.h>
#import <pthread.h>

static double gProcStart;
static const double kScale = 0.5;          // pixels per point of the sampled image
static const double kMinBand = 60;         // pt: a shorter uniform run of rows is spacing, not a blank band

typedef struct { int bubbles, inked, partial, empty, unpainted; double blankPt, missing; } Health;

static double now(void) { return CACurrentMediaTime(); }
static double cpuMs(void) { struct rusage u; getrusage(RUSAGE_SELF, &u); return (u.ru_utime.tv_sec + u.ru_stime.tv_sec) * 1e3 + (u.ru_utime.tv_usec + u.ru_stime.tv_usec) / 1e3; }
static double mainCpuMs(void) {
  thread_basic_info_data_t info; mach_msg_type_number_t n = THREAD_BASIC_INFO_COUNT;
  if (thread_info(pthread_mach_thread_np(pthread_self()), THREAD_BASIC_INFO, (thread_info_t)&info, &n) != KERN_SUCCESS) return -1;
  return (info.user_time.seconds + info.system_time.seconds) * 1e3 + (info.user_time.microseconds + info.system_time.microseconds) / 1e3;
}

// 1 wallpaper, 2 my bubble, 3 their bubble (and day pills), 0 anything else: ink. The wallpaper is a flat #efeae2
// or a two-tone doodle between #efe7dd and #e2d7c9, antialiased edges included.
static inline int classOf(const uint8_t *p) {
  int r = p[0], g = p[1], b = p[2];
  if (abs(r - 0xef) <= 6 && abs(g - 0xea) <= 6 && abs(b - 0xe2) <= 6) return 1;
  if (r >= 0xe2 - 3 && r <= 0xef + 3) {
    double t = (0xef - r) / 13.0; // along the doodle's blend, by red
    if (fabs(g - (0xe7 - 16 * t)) <= 4 && fabs(b - (0xdd - 20 * t)) <= 4) return 1;
  }
  if (abs(r - 0xd9) <= 6 && abs(g - 0xfd) <= 6 && abs(b - 0xd3) <= 6) return 2;
  if (r >= 249 && g >= 249 && b >= 249) return 3;
  return 0;
}

// Classifies one rendered frame. A bubble is a 4-connected region of one bubble colour at least 20 × 16 pt, with
// wallpaper beside it. It is `unpainted` when it is photo-sized (at least 200 × 140 pt) and the middle 60 % of its
// width is bubble colour for 60 pt of rows or more: the photo is not drawn. Otherwise it is `empty` when under 0.6 %
// of its pixels are ink: no text, time or waveform. A blank band is a run of at least 60 pt of rows that hold no
// bubble: only wallpaper, or one flat colour (luminance spanning under 10 levels). `missing` is the area of empty
// and unpainted bubbles and blank bands over the visible area.
static Health analyze(const uint8_t *px, size_t W, size_t H) {
  Health h = {0};
  uint8_t *cls = malloc(W * H); int32_t *stack = malloc(sizeof(int32_t) * W * H); uint8_t *seen = calloc(W * H, 1);
  for (size_t i = 0; i < W * H; i++) cls[i] = (uint8_t)classOf(px + i * 4);
  double missingPx = 0;
  for (size_t start = 0; start < W * H; start++) {
    int c = cls[start];
    if ((c != 2 && c != 3) || seen[start]) continue;
    size_t sp = 0, area = 0, x0 = W, x1 = 0, y0 = H, y1 = 0;
    stack[sp++] = (int32_t)start; seen[start] = 1;
    while (sp) {
      size_t i = (size_t)stack[--sp], x = i % W, y = i / W; area++;
      if (x < x0) x0 = x; if (x > x1) x1 = x; if (y < y0) y0 = y; if (y > y1) y1 = y;
      size_t nb[4] = { x > 0 ? i - 1 : SIZE_MAX, x + 1 < W ? i + 1 : SIZE_MAX, y > 0 ? i - W : SIZE_MAX, y + 1 < H ? i + W : SIZE_MAX };
      for (int k = 0; k < 4; k++) if (nb[k] != SIZE_MAX && !seen[nb[k]] && cls[nb[k]] == c) { seen[nb[k]] = 1; stack[sp++] = (int32_t)nb[k]; }
    }
    size_t bw = x1 - x0 + 1, bh = y1 - y0 + 1;
    if (bw < 20 * kScale || bh < 16 * kScale) continue;
    // A bubble sits on the wallpaper (or, for a photo's placeholder, inside the other bubble colour): most of the
    // columns 2 px left and right of it are those. A bright patch inside a photo is surrounded by the photo.
    size_t beside = 0, on = 0;
    for (size_t y = y0; y <= y1; y++) for (int side = 0; side < 2; side++) {
      if (side == 0 ? x0 < 2 : x1 + 2 >= W) { beside++; on++; continue; }
      int k = cls[y * W + (side == 0 ? x0 - 2 : x1 + 2)]; beside++; // 2 px out: past an antialiased edge
      if (k == 1 || (k != 0 && k != c)) on++;
    }
    if (on * 2 < beside) continue;
    h.bubbles++;
    size_t ink = 0;
    // Ink is text, waveforms, photos: a pixel of neither bubble colour nor wallpaper that is darker than an
    // antialiased edge between them, or that has no bubble or wallpaper pixel beside it (a pale photo).
    for (size_t y = y0; y <= y1; y++) for (size_t x = x0; x <= x1; x++) {
      size_t i = y * W + x; const uint8_t *p = px + i * 4;
      if (cls[i] != 0) continue;
      if ((p[0] * 3 + p[1] * 6 + p[2]) / 10 < 215) { ink++; continue; }
      if (x > 0 && x + 1 < W && y > 0 && y + 1 < H && !cls[i - 1] && !cls[i + 1] && !cls[i - W] && !cls[i + W]) ink++;
    }
    // Cut by the top or bottom of the list, a bubble is judged only when 60 pt of it shows: a strip of a bubble
    // may hold nothing but its padding. It is `partial`: neither drawn nor missing.
    if ((y0 == 0 || y1 + 1 == H) && bh < 60 * kScale) { h.partial++; continue; }
    size_t hole = 0, run = 0;
    if (bw >= 200 * kScale && bh >= 140 * kScale) {
      size_t a = x0 + bw / 5, b = x1 - bw / 5;
      for (size_t y = y0; y <= y1; y++) {
        BOOL flat = YES;
        for (size_t x = a; x <= b && flat; x++) flat = cls[y * W + x] == c;
        run = flat ? run + 1 : 0; if (run > hole) hole = run;
      }
    }
    if (hole >= 60 * kScale) { h.unpainted++; missingPx += bw * bh; }
    else if (ink < 0.006 * area) { h.empty++; missingPx += bw * bh; }
    else h.inked++;
  }
  size_t m = (size_t)(12 * kScale), run = 0;
  for (size_t y = 0; y <= H; y++) {
    BOOL uniform = NO;
    if (y < H) {
      int lo = 255, hi = 0; BOOL bubble = NO, ink = NO;
      for (size_t x = m; x + m < W; x++) {
        const uint8_t *p = px + (y * W + x) * 4; int l = (p[0] * 3 + p[1] * 6 + p[2]) / 10, k = cls[y * W + x];
        if (l < lo) lo = l; if (l > hi) hi = l; if (k == 2 || k == 3) bubble = YES; if (k == 0) ink = YES;
      }
      uniform = !bubble && (!ink || hi - lo < 10);
    }
    if (uniform) run++;
    else { if (run >= kMinBand * kScale) { h.blankPt += run / kScale; missingPx += run * W; } run = 0; }
  }
  h.missing = fmin(1, missingPx / (double)(W * H));
  free(cls); free(stack); free(seen);
  return h;
}

@interface ChatProbe : NSObject
@property (nonatomic, strong) CADisplayLink *link;
@property (nonatomic, weak) UIScrollView *scroll;
@property (nonatomic, copy) NSString *phase;
@property (nonatomic) NSInteger frame, dumped, sample;
@property (nonatomic) double phaseStart, lastTick, edgeSince, firstContent, ready, firstScroll, sampleCost, cpu0, main0;
@property (nonatomic) CGFloat edgeHeight;
@property (nonatomic) double edgePosted;
@property (nonatomic) double lastEdge, edgeCpu, edgeMain;
@property (nonatomic) BOOL dumpedReady;
@property (nonatomic) double touched, touchedCpu, touchedMain; // a pan's start (and the CPU then), until a tick takes it
@property (nonatomic, strong) NSMutableArray *frames, *samples, *phases;
@property (nonatomic, strong) NSMutableDictionary *result;
@end

@implementation ChatProbe

- (UIScrollView *)findScroll {
  UIScrollView *best = nil; NSMutableArray *queue = [NSMutableArray array];
  for (UIWindowScene *s in UIApplication.sharedApplication.connectedScenes)
    if ([s isKindOfClass:UIWindowScene.class]) for (UIWindow *w in s.windows) [queue addObject:w];
  while (queue.count) {
    UIView *v = queue.lastObject; [queue removeLastObject];
    if (v.hidden || v.alpha < 0.01) continue;
    if ([v isKindOfClass:UIScrollView.class]) {
      UIScrollView *s = (UIScrollView *)v;
      if (s.contentSize.height > s.bounds.size.height * 1.5 && (!best || s.bounds.size.height > best.bounds.size.height)) best = s;
    }
    [queue addObjectsFromArray:v.subviews];
  }
  return best;
}

// A list drawn upside down (React Native's inverted lists flip the scroll view) has its visual top at the end.
static BOOL flipped(UIView *v) {
  int flips = 0;
  for (; v; v = v.superview) if (v.transform.d < 0 || v.layer.transform.m22 < 0) flips++;
  return flips % 2 == 1;
}

- (BOOL)atVisualTop {
  UIScrollView *s = self.scroll; UIEdgeInsets in = s.adjustedContentInset;
  if (flipped(s)) return s.contentOffset.y >= s.contentSize.height - s.bounds.size.height + in.bottom - 1;
  return s.contentOffset.y <= -in.top + 1;
}

- (Health)sampleAt:(double)t {
  UIScrollView *s = self.scroll; UIWindow *w = s.window;
  Health h = { .bubbles = -1 };
  if (!w) return h;
  UIEdgeInsets in = s.adjustedContentInset;
  CGRect vis = s.bounds; vis.origin.y += in.top; vis.size.height -= in.top + in.bottom;
  // Past the content's ends (a rubber-band overscroll) there is nothing to draw.
  vis = CGRectIntersection(vis, CGRectMake(vis.origin.x, 0, vis.size.width, s.contentSize.height));
  if (CGRectIsNull(vis)) return h;
  CGRect r = CGRectIntersection([s convertRect:vis toView:w], w.bounds);
  size_t W = (size_t)(r.size.width * kScale), H = (size_t)(r.size.height * kScale);
  if (W < 16 || H < 16) return h;
  uint8_t *px = calloc(W * H * 4, 1);
  CGColorSpaceRef cs = CGColorSpaceCreateDeviceRGB();
  CGContextRef ctx = CGBitmapContextCreate(px, W, H, 8, W * 4, cs, (CGBitmapInfo)kCGImageAlphaPremultipliedLast);
  CGColorSpaceRelease(cs);
  CGContextTranslateCTM(ctx, 0, H); CGContextScaleCTM(ctx, kScale, -kScale); CGContextTranslateCTM(ctx, -r.origin.x, -r.origin.y);
  double t0 = now();
  [w.layer renderInContext:ctx];
  h = analyze(px, W, H);
  self.sampleCost += now() - t0;
  NSString *dir = NSProcessInfo.processInfo.environment[@"CHAT_DUMP"];
  BOOL bad = h.empty || h.unpainted || h.blankPt > 0, firstReady = !bad && !self.dumpedReady && h.inked > 0;
  if (dir.length && ((bad && self.dumped < 40) || firstReady)) {
    if (firstReady) self.dumpedReady = YES;
    [NSFileManager.defaultManager createDirectoryAtPath:dir withIntermediateDirectories:YES attributes:nil error:nil];
    CGImageRef im = CGBitmapContextCreateImage(ctx);
    NSString *name = [NSString stringWithFormat:@"%@/%d-%@-%05.0f-b%d-e%d-u%d-z%.0f.png", dir, getpid(), self.phase, t * 1000, h.bubbles, h.empty, h.unpainted, h.blankPt];
    [UIImagePNGRepresentation([UIImage imageWithCGImage:im]) writeToFile:name atomically:YES];
    CGImageRelease(im); if (bad) self.dumped++;
  }
  CGContextRelease(ctx); free(px);
  return h;
}

- (void)begin:(NSString *)phase at:(double)t {
  [self begin:phase at:t cpu:cpuMs() main:mainCpuMs()];
}

// `cpu` and `main` are the process's and the main thread's CPU at `t`, read when the boundary was crossed.
- (void)begin:(NSString *)phase at:(double)t cpu:(double)cpu main:(double)main {
  if (self.phase) [self.phases addObject:@{ @"phase": self.phase, @"start": @(self.phaseStart * 1000), @"end": @(t * 1000),
                                            @"cpuMs": @(cpu - self.cpu0), @"mainCpuMs": @(main - self.main0) }];
  self.phase = phase; self.phaseStart = t; self.cpu0 = cpu; self.main0 = main;
}

- (void)panned:(UIPanGestureRecognizer *)g {
  if (g.state == UIGestureRecognizerStateBegan && self.touched == 0) { self.touched = now() - gProcStart; self.touchedCpu = cpuMs(); self.touchedMain = mainCpuMs(); }
}

- (void)tick:(CADisplayLink *)l {
  // Every time here is when this frame shows (`targetTimestamp`): what the probe renders now is that frame's layer
  // tree. The interval between frames is measured between the callbacks' `timestamp`s.
  double t = l.targetTimestamp - gProcStart, tick = l.timestamp - gProcStart; self.frame++;
  if (!self.scroll || !self.scroll.window) {
    self.scroll = [self findScroll];
    if (self.scroll && self.firstScroll < 0) self.firstScroll = t;
    [self.scroll.panGestureRecognizer addTarget:self action:@selector(panned:)];
  }
  UIScrollView *s = self.scroll;
  // A pan that began and ended between two callbacks (a hitch) still starts the phase, at its own time.
  double touch = self.touched; self.touched = 0;
  if (touch > 0 && [self.phase isEqualToString:@"launch"]) [self begin:@"up" at:touch cpu:self.touchedCpu main:self.touchedMain];
  if (touch > 0 && [self.phase isEqualToString:@"rest"]) [self begin:@"flings" at:touch cpu:self.touchedCpu main:self.touchedMain];
  BOOL scrolling = ![self.phase isEqualToString:@"launch"];
  if (scrolling) // interval since the previous frame, and the interval this frame was meant to have
    [self.frames addObject:@[@(round(t * 1e4) / 10), @(round((tick - self.lastTick) * 1e4) / 10), @(round((l.targetTimestamp - l.timestamp) * 1e4) / 10),
                             @(round(s.contentOffset.y)), @(round(s.contentSize.height))]];
  self.lastTick = tick;
  BOOL launching = !scrolling && s && (self.ready < 0 || t < self.ready + 1);
  if (s && (launching || (scrolling && self.sample > 0 && self.frame % self.sample == 0))) {
    Health h = [self sampleAt:t];
    if (h.bubbles >= 0) {
      [self.samples addObject:@[@(round(t * 1e4) / 10), self.phase, @(h.bubbles), @(h.inked), @(h.empty), @(h.unpainted), @(h.blankPt), @(round(h.missing * 1e4) / 1e4),
                                @(round(s.contentOffset.y)), @(h.partial)]];
      if (h.inked > 0 && self.firstContent < 0) self.firstContent = t;
      if (!scrolling && self.ready < 0 && h.inked >= 3 && !h.empty && !h.unpainted && h.blankPt == 0) self.ready = t;
    }
  }
  if ([self.phase isEqualToString:@"up"] && s) {
    if ([self atVisualTop] && s.contentSize.height == self.edgeHeight) {
      // At the top 500 ms, then again each second it stays there: a check that ran before message 1's texts
      // reached the accessibility tree is asked again.
      if (self.edgeSince < 0) { self.edgeSince = t; self.edgePosted = -1; self.edgeCpu = cpuMs(); self.edgeMain = mainCpuMs(); }
      else if (t - self.edgeSince >= 0.5 && (self.edgePosted < 0 || t - self.edgePosted >= 1)) {
        self.edgePosted = t; self.lastEdge = self.edgeSince; notify_post("dev.exact.chatreplay.edge");
      }
    } else self.edgeSince = -1;
    self.edgeHeight = s.contentSize.height;
  }
}

- (void)finish {
  double t = now() - gProcStart;
  [self.link invalidate];
  [self begin:@"done" at:t];
  NSMutableDictionary *o = self.result;
  o[@"bundle"] = NSBundle.mainBundle.bundleIdentifier ?: @"";
  o[@"maxFps"] = @(UIScreen.mainScreen.maximumFramesPerSecond);
  id (^ms)(double) = ^id(double v) { return v < 0 ? (id)NSNull.null : @(v * 1000); }; // never seen: null
  o[@"firstScrollViewMs"] = ms(self.firstScroll); o[@"firstContentMs"] = ms(self.firstContent); o[@"readyMs"] = ms(self.ready);
  o[@"sample"] = @(self.sample); o[@"sampleCostMs"] = @(self.sampleCost * 1000);
  o[@"phases"] = self.phases; o[@"frames"] = self.frames; o[@"samples"] = self.samples;
  o[@"frameColumns"] = @[@"t", @"dt", @"expected", @"offset", @"contentHeight"];
  o[@"sampleColumns"] = @[@"t", @"phase", @"bubbles", @"inked", @"empty", @"unpainted", @"blankPt", @"missing", @"offset", @"partial"];
  NSString *out = NSProcessInfo.processInfo.environment[@"CHAT_OUT"];
  if (!out.length) out = [NSSearchPathForDirectoriesInDomains(NSDocumentDirectory, NSUserDomainMask, YES).firstObject stringByAppendingPathComponent:@"chatprobe.json"];
  NSData *d = [NSJSONSerialization dataWithJSONObject:o options:0 error:nil];
  NSString *tmp = [out stringByAppendingString:@".tmp"];
  if (d && [d writeToFile:tmp atomically:YES]) rename(tmp.fileSystemRepresentation, out.fileSystemRepresentation);
}

- (void)start {
  self.frames = [NSMutableArray array]; self.samples = [NSMutableArray array]; self.phases = [NSMutableArray array];
  self.result = [NSMutableDictionary dictionary];
  self.firstContent = self.ready = self.firstScroll = self.edgeSince = -1;
  NSString *n = NSProcessInfo.processInfo.environment[@"CHAT_SAMPLE"];
  self.sample = n ? n.integerValue : 2;
  [self begin:@"launch" at:now() - gProcStart];
  self.link = [CADisplayLink displayLinkWithTarget:self selector:@selector(tick:)];
  self.link.preferredFrameRateRange = CAFrameRateRangeMake(80, 120, 120);
  [self.link addToRunLoop:NSRunLoop.mainRunLoop forMode:NSRunLoopCommonModes];
  int token;
  notify_register_dispatch("dev.exact.chatreplay.done", &token, dispatch_get_main_queue(), ^(int _) { [self finish]; });
  // Message 1 confirmed at the edge last posted: `up` ends where that rest began, with the CPU read there. If the
  // list has left that edge since (a page landed), the confirmation is refused and the runner flicks on. The rest
  // at the top until the first fling is `rest`, which no metric counts; samples taken in it are relabelled.
  notify_register_dispatch("dev.exact.chatreplay.top", &token, dispatch_get_main_queue(), ^(int _) {
    if (![self.phase isEqualToString:@"up"]) return;
    UIScrollView *s = self.scroll;
    if (self.edgeSince != self.lastEdge || !s || ![self atVisualTop] || s.contentSize.height != self.edgeHeight) {
      notify_post("dev.exact.chatreplay.notop"); return;
    }
    notify_post("dev.exact.chatreplay.topok");
    self.touched = 0; // an up flick's pan not yet taken does not start the flings
    self.result[@"topMs"] = @((self.lastEdge - self.phaseStart) * 1000);
    [self begin:@"rest" at:self.lastEdge cpu:self.edgeCpu main:self.edgeMain];
    for (NSUInteger i = 0; i < self.samples.count; i++) {
      NSArray *x = self.samples[i];
      if ([x[1] isEqualToString:@"up"] && [x[0] doubleValue] >= self.lastEdge * 1000) {
        NSMutableArray *m = [x mutableCopy]; m[1] = @"rest"; self.samples[i] = m;
      }
    }
  });
}
@end

static ChatProbe *gProbe;

__attribute__((constructor)) static void chatprobeInit(void) {
  // Process start: kinfo_proc.p_starttime is wall-clock; map it onto the media clock.
  double media = now(); gProcStart = media;
  struct kinfo_proc kp; size_t len = sizeof kp; int mib[4] = { CTL_KERN, KERN_PROC, KERN_PROC_PID, getpid() };
  if (sysctl(mib, 4, &kp, &len, NULL, 0) == 0 && kp.kp_proc.p_starttime.tv_sec > 0) {
    struct timeval w; gettimeofday(&w, NULL);
    double ago = (w.tv_sec - kp.kp_proc.p_starttime.tv_sec) + (w.tv_usec - kp.kp_proc.p_starttime.tv_usec) / 1e6;
    if (ago >= 0 && ago < 60) gProcStart = media - ago;
  }
  dispatch_async(dispatch_get_main_queue(), ^{ gProbe = [ChatProbe new]; [gProbe start]; });
}
