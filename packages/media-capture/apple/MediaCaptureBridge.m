// @ref LLP 1156.008.001 A3 — Public-API Objective-C @try/@catch bridge for AVAudioEngine.
#import "MediaCaptureBridge.h"

#if (defined(__MAC_OS_X_VERSION_MAX_ALLOWED) && __MAC_OS_X_VERSION_MAX_ALLOWED >= 270000) \
    || (defined(__IPHONE_OS_VERSION_MAX_ALLOWED) && __IPHONE_OS_VERSION_MAX_ALLOWED >= 270000) \
    || (defined(__TV_OS_VERSION_MAX_ALLOWED) && __TV_OS_VERSION_MAX_ALLOWED >= 270000)
#define EXACT_MEDIA_CAPTURE_SDK_27 1
#else
#define EXACT_MEDIA_CAPTURE_SDK_27 0
#endif

#if !__has_feature(objc_arc)
#error "MediaCaptureBridge.m must be compiled with -fobjc-arc -fobjc-arc-exceptions"
#endif

NSErrorDomain const ExactMediaCaptureExceptionDomain = @"dev.exact.media-capture.objc-exception";

static NSError *ExactExceptionError(NSException *exception) {
    NSString *name = exception.name ?: @"NSException";
    NSString *reason = exception.reason ?: @"no reason given";
    NSString *description = [NSString stringWithFormat:@"AVAudioEngine raised %@: %@", name, reason];
    return [NSError errorWithDomain:ExactMediaCaptureExceptionDomain
                               code:1
                           userInfo:@{ NSLocalizedDescriptionKey: description }];
}

@implementation ExactMediaCaptureBridge

+ (nullable AVAudioInputNode *)inputNodeForEngine:(AVAudioEngine *)engine error:(NSError * _Nullable * _Nullable)error {
    @try {
        return engine.inputNode;
    } @catch (NSException *exception) {
        if (error) *error = ExactExceptionError(exception);
        return nil;
    }
}

+ (nullable NSNumber *)voiceProcessingEnabledForEngine:(AVAudioEngine *)engine error:(NSError * _Nullable * _Nullable)error {
    @try {
        return @(engine.inputNode.isVoiceProcessingEnabled);
    } @catch (NSException *exception) {
        if (error) *error = ExactExceptionError(exception);
        return nil;
    }
}

+ (BOOL)setVoiceProcessing:(BOOL)enabled forEngine:(AVAudioEngine *)engine error:(NSError * _Nullable * _Nullable)error {
    @try {
        return [engine.inputNode setVoiceProcessingEnabled:enabled error:error];
    } @catch (NSException *exception) {
        if (error) *error = ExactExceptionError(exception);
        return NO;
    }
}

+ (nullable AVAudioFormat *)outputFormatForEngine:(AVAudioEngine *)engine bus:(AVAudioNodeBus)bus error:(NSError * _Nullable * _Nullable)error {
    @try {
        return [engine.inputNode outputFormatForBus:bus];
    } @catch (NSException *exception) {
        if (error) *error = ExactExceptionError(exception);
        return nil;
    }
}

+ (BOOL)installTapOnEngine:(AVAudioEngine *)engine
                       bus:(AVAudioNodeBus)bus
                bufferSize:(AVAudioFrameCount)bufferSize
                    format:(nullable AVAudioFormat *)format
                     block:(AVAudioNodeTapBlock)block
                     error:(NSError * _Nullable * _Nullable)error {
    @try {
        AVAudioInputNode *input = engine.inputNode;
#if EXACT_MEDIA_CAPTURE_SDK_27
        if (@available(macOS 27.0, iOS 27.0, tvOS 27.0, *)) {
            return [input installTapOnBus:bus bufferSize:bufferSize format:format error:error block:block];
        }
#endif
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
        [input installTapOnBus:bus bufferSize:bufferSize format:format block:block];
#pragma clang diagnostic pop
        return YES;
    } @catch (NSException *exception) {
        if (error) *error = ExactExceptionError(exception);
        return NO;
    }
}

+ (BOOL)removeTapOnEngine:(AVAudioEngine *)engine bus:(AVAudioNodeBus)bus error:(NSError * _Nullable * _Nullable)error {
    @try {
        [engine.inputNode removeTapOnBus:bus];
        return YES;
    } @catch (NSException *exception) {
        if (error) *error = ExactExceptionError(exception);
        return NO;
    }
}

+ (BOOL)attachAndConnectMixer:(AVAudioMixerNode *)mixer
                     onEngine:(AVAudioEngine *)engine
                        input:(AVAudioInputNode *)input
                       format:(AVAudioFormat *)format
                        error:(NSError * _Nullable * _Nullable)error {
    @try {
        [engine attachNode:mixer];
        AVAudioMixerNode *mainMixer = engine.mainMixerNode;
#if EXACT_MEDIA_CAPTURE_SDK_27
        if (@available(macOS 27.0, iOS 27.0, tvOS 27.0, *)) {
            if (![engine connect:input to:mixer format:format error:error]) return NO;
            if (![engine connect:mixer to:mainMixer format:format error:error]) return NO;
            return YES;
        }
#endif
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
        [engine connect:input to:mixer format:format];
        [engine connect:mixer to:mainMixer format:format];
#pragma clang diagnostic pop
        return YES;
    } @catch (NSException *exception) {
        if (error) *error = ExactExceptionError(exception);
        return NO;
    }
}

+ (BOOL)prepareAndStartEngine:(AVAudioEngine *)engine error:(NSError * _Nullable * _Nullable)error {
    @try {
        [engine prepare];
        return [engine startAndReturnError:error];
    } @catch (NSException *exception) {
        if (error) *error = ExactExceptionError(exception);
        return NO;
    }
}

+ (void)stopEngine:(AVAudioEngine *)engine {
    @try {
        [engine.inputNode removeTapOnBus:0];
    } @catch (NSException *exception) {
    }
    @try {
        [engine stop];
    } @catch (NSException *exception) {
    }
}

+ (BOOL)catchObjCException:(NS_NOESCAPE void (^)(void))block error:(NSError * _Nullable * _Nullable)error {
    @try {
        block();
        return YES;
    } @catch (NSException *exception) {
        if (error) *error = ExactExceptionError(exception);
        return NO;
    }
}

@end
