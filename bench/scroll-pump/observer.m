#import <UIKit/UIKit.h>
#import <QuartzCore/QuartzCore.h>
#import <objc/runtime.h>
#include <stdio.h>

static IMP originalSendEvent, originalMakeLink, originalAdd, originalPause, originalInvalidate;
static NSHashTable<CADisplayLink *> *appLinks;
static NSHashTable<CADisplayLink *> *mainLinks;
static NSString *appImagePrefix;
static BOOL recording;
static double starts[512];
static NSUInteger startsWritten, revealAfter;
static const double quietInterval = .11;

static void recordStart(void) {
    if (!recording) return;
    if (startsWritten < 512) starts[startsWritten] = CACurrentMediaTime();
    startsWritten++;
}

static CADisplayLink *makeLink(Class type, SEL method, id target, SEL action) {
    CADisplayLink *link = ((CADisplayLink *(*)(Class, SEL, id, SEL))originalMakeLink)(type, method, target, action);
    if (NSThread.isMainThread && target) {
        const char *image = class_getImageName(object_getClass(target));
        if (image && [[NSString stringWithUTF8String:image].stringByResolvingSymlinksInPath hasPrefix:appImagePrefix]) {
            [appLinks addObject:link];
        }
    }
    return link;
}

static void addToRunLoop(CADisplayLink *link, SEL method, NSRunLoop *loop, NSRunLoopMode mode) {
    if (NSThread.isMainThread && loop == NSRunLoop.mainRunLoop && [appLinks containsObject:link] && ![mainLinks containsObject:link]) {
        [mainLinks addObject:link];
        if (!link.isPaused) recordStart();
    }
    ((void (*)(CADisplayLink *, SEL, NSRunLoop *, NSRunLoopMode))originalAdd)(link, method, loop, mode);
}

static void setPaused(CADisplayLink *link, SEL method, BOOL paused) {
    if (recording && !paused && NSThread.isMainThread && link.isPaused && [mainLinks containsObject:link]) {
        recordStart();
    }
    ((void (*)(CADisplayLink *, SEL, BOOL))originalPause)(link, method, paused);
}

static void invalidate(CADisplayLink *link, SEL method) {
    if (NSThread.isMainThread) [mainLinks removeObject:link];
    ((void (*)(CADisplayLink *, SEL))originalInvalidate)(link, method);
}

static NSUInteger activeLinks(void) {
    NSUInteger active = 0;
    for (CADisplayLink *link in mainLinks) if (!link.isPaused) active++;
    return active;
}

typedef struct { double time, offset; BOOL decelerating; } OffsetSample;

@interface ScrollObserver : NSObject
- (instancetype)initWithPath:(NSString *)path;
- (void)event:(UIEvent *)event;
@end

@implementation ScrollObserver {
    UIScrollView *_scroll;
    __weak UITouch *_touch;
    NSString *_path;
    OffsetSample _samples[512];
    NSUInteger _written, _activeAtDown, _completed;
    CGPoint _downPoint, _releasePoint;
    CGRect _frame;
    double _release, _downOffset, _releaseOffset, _low, _high;
    double _restEdge, _lastOffset, _lastMovement;
    BOOL _cancelled, _overshot;
    NSTimer *_quietTimer, *_watchdog;
}

- (instancetype)initWithPath:(NSString *)path {
    if ((self = [super init])) _path = [path copy];
    return self;
}

static void horizontalLimits(UIScrollView *scroll, double *low, double *high) {
    UIEdgeInsets inset = scroll.adjustedContentInset;
    *low = -inset.left;
    *high = fmax(*low, scroll.contentSize.width - scroll.bounds.size.width + inset.right);
}

- (void)observeValueForKeyPath:(NSString *)keyPath ofObject:(id)object
                       change:(NSDictionary *)change context:(void *)context {
    if (object != _scroll) return;
    OffsetSample sample = {CACurrentMediaTime(), _scroll.contentOffset.x, _scroll.isDecelerating};
    if (_written < 512) _samples[_written] = sample;
    _written++;
    if (fabs(sample.offset - _lastOffset) > .01) _lastMovement = sample.time;
    _lastOffset = sample.offset;
    if (!_release) return;
    if (sample.decelerating && (sample.offset < _low - 1 || sample.offset > _high + 1)) _overshot = YES;
    if (_overshot && !_quietTimer && fabs(sample.offset - _restEdge) <= .6) [self checkAfter:quietInterval];
}

- (void)begin:(UITouch *)touch {
    if (_scroll || !touch.window) return;
    CGPoint point = [touch locationInView:touch.window];
    UIView *hit = touch.view ?: [touch.window hitTest:point withEvent:nil];
    for (UIView *view = hit; view; view = view.superview) {
        if (![view isKindOfClass:UIScrollView.class]) continue;
        UIScrollView *scroll = (UIScrollView *)view;
        double low, high;
        horizontalLimits(scroll, &low, &high);
        if (!scroll.isScrollEnabled || high - low < 40 || view.bounds.size.height > 220) continue;
        _scroll = scroll;
        _touch = touch;
        _written = startsWritten = 0;
        _release = 0;
        _overshot = NO;
        _activeAtDown = activeLinks();
        recording = YES;
        _low = low; _high = high;
        _frame = [scroll convertRect:scroll.bounds toView:touch.window];
        _downOffset = scroll.contentOffset.x;
        _lastOffset = _downOffset;
        _lastMovement = CACurrentMediaTime();
        _restEdge = fabs(_downOffset - low) <= fabs(_downOffset - high) ? high : low;
        _downPoint = point;
        [_scroll addObserver:self forKeyPath:@"contentOffset" options:0 context:NULL];
        break;
    }
}

- (BOOL)idle {
    return !(_scroll.isDecelerating || _scroll.isDragging || _scroll.isTracking);
}

- (NSTimer *)after:(double)delay action:(SEL)action {
    NSTimer *timer = [NSTimer timerWithTimeInterval:delay target:self selector:action userInfo:nil repeats:NO];
    [NSRunLoop.mainRunLoop addTimer:timer forMode:NSRunLoopCommonModes];
    return timer;
}

- (void)checkAfter:(double)delay {
    _quietTimer = [self after:delay action:@selector(checkRest:)];
}

- (void)checkRest:(NSTimer *)timer {
    _quietTimer = nil;
    double remaining = quietInterval - (CACurrentMediaTime() - _lastMovement);
    if (remaining <= 0 && self.idle && fabs(_lastOffset - _restEdge) <= .6) [self finish];
    else [self checkAfter:fmax(.05, remaining)];
}

- (void)motionTimedOut:(NSTimer *)timer {
    [self finish];
}

// Only the horizontal rebound is under test. Use UIKit's scroll setter for the title assertion.
- (void)revealTitleBelow:(UIScrollView *)strip {
    for (UIView *view = strip.superview; view; view = view.superview) {
        if (![view isKindOfClass:UIScrollView.class]) continue;
        UIScrollView *outer = (UIScrollView *)view;
        UIEdgeInsets inset = outer.adjustedContentInset;
        CGFloat bottom = outer.contentSize.height - outer.bounds.size.height + inset.bottom;
        if (bottom <= -inset.top) continue;
        [outer setContentOffset:CGPointMake(outer.contentOffset.x, bottom) animated:NO];
        return;
    }
}

- (void)finish {
    double restTime = CACurrentMediaTime();
    [_quietTimer invalidate];
    [_watchdog invalidate];
    _quietTimer = _watchdog = nil;
    [_scroll removeObserver:self forKeyPath:@"contentOffset"];
    recording = NO;
    NSMutableArray *samples = [NSMutableArray arrayWithCapacity:MIN(_written, 512)];
    for (NSUInteger i = 0; i < MIN(_written, 512); i++) {
        OffsetSample row = _samples[i];
        [samples addObject:@[@(row.time), @(row.offset), @(row.decelerating)]];
    }
    NSMutableArray *activations = [NSMutableArray arrayWithCapacity:MIN(startsWritten, 512)];
    for (NSUInteger i = 0; i < MIN(startsWritten, 512); i++) [activations addObject:@(starts[i])];
    double low, high;
    horizontalLimits(_scroll, &low, &high);
    NSDictionary *record = @{
        @"starts": activations, @"startsWritten": @(startsWritten),
        @"activeAtDown": @(_activeAtDown), @"activeAtRest": @(activeLinks()),
        @"cancelled": @(_cancelled), @"release": @(_release), @"restTime": @(restTime),
        @"downPoint": @[@(_downPoint.x), @(_downPoint.y)], @"releasePoint": @[@(_releasePoint.x), @(_releasePoint.y)],
        @"frame": @[@(_frame.origin.x), @(_frame.origin.y), @(_frame.size.width), @(_frame.size.height)],
        @"bounds": @[@(_low), @(_high)], @"restBounds": @[@(low), @(high)],
        @"initial": @(_downOffset), @"atRelease": @(_releaseOffset), @"rest": @(_scroll.contentOffset.x),
        @"idle": @(self.idle),
        @"written": @(_written), @"samples": samples,
    };
    NSData *json = [NSJSONSerialization dataWithJSONObject:record options:0 error:NULL];
    FILE *file = fopen(_path.fileSystemRepresentation, "a");
    if (file && json) {
        fwrite(json.bytes, 1, json.length, file);
        fputc('\n', file);
    }
    if (file) fclose(file);
    UIScrollView *strip = _scroll;
    _scroll = nil;
    _touch = nil;
    if (++_completed == revealAfter) [self revealTitleBelow:strip];
}

- (void)event:(UIEvent *)event {
    if (event.type != UIEventTypeTouches) return;
    for (UITouch *touch in event.allTouches) {
        if (touch.phase == UITouchPhaseBegan) [self begin:touch];
        if (_touch == touch && !_release && (touch.phase == UITouchPhaseEnded || touch.phase == UITouchPhaseCancelled)) {
            _cancelled = touch.phase == UITouchPhaseCancelled;
            _release = CACurrentMediaTime();
            _releaseOffset = _scroll.contentOffset.x;
            _releasePoint = [touch locationInView:_scroll.window];
            _watchdog = [self after:3 action:@selector(motionTimedOut:)];
        }
    }
}
@end

static ScrollObserver *observer;

static void sendEvent(UIApplication *app, SEL method, UIEvent *event) {
    [observer event:event];
    ((void (*)(UIApplication *, SEL, UIEvent *))originalSendEvent)(app, method, event);
}

static IMP replace(Class class, SEL selector, IMP replacement) {
    return method_setImplementation(class_getInstanceMethod(class, selector), replacement);
}

// UIKit drives its own rebound in this process. Track links whose target comes from the app bundle.
__attribute__((constructor)) static void install(void) {
    const char *path = getenv("SCROLL_PROBE_RECORD");
    if (!path) return;
    const char *reveal = getenv("SCROLL_PROBE_REVEAL_AFTER");
    if (reveal) revealAfter = strtoul(reveal, NULL, 10);
    @autoreleasepool {
        appImagePrefix = [NSBundle.mainBundle.bundlePath.stringByResolvingSymlinksInPath stringByAppendingString:@"/"];
        appLinks = NSHashTable.weakObjectsHashTable;
        mainLinks = NSHashTable.weakObjectsHashTable;
        observer = [[ScrollObserver alloc] initWithPath:[NSString stringWithUTF8String:path]];
        originalSendEvent = replace(UIApplication.class, @selector(sendEvent:), (IMP)sendEvent);
        originalMakeLink = method_setImplementation(class_getClassMethod(CADisplayLink.class, @selector(displayLinkWithTarget:selector:)), (IMP)makeLink);
        originalAdd = replace(CADisplayLink.class, @selector(addToRunLoop:forMode:), (IMP)addToRunLoop);
        originalPause = replace(CADisplayLink.class, @selector(setPaused:), (IMP)setPaused);
        originalInvalidate = replace(CADisplayLink.class, @selector(invalidate), (IMP)invalidate);
    }
}
