// @ref LLP 1156.008.001 A3 — Public-API Objective-C @try/@catch bridge for AVAudioEngine.
#import <AVFAudio/AVFAudio.h>
#import <Foundation/Foundation.h>

NS_ASSUME_NONNULL_BEGIN

FOUNDATION_EXPORT NSErrorDomain const ExactMediaCaptureExceptionDomain;

@interface ExactMediaCaptureBridge : NSObject

+ (nullable AVAudioInputNode *)inputNodeForEngine:(AVAudioEngine *)engine error:(NSError * _Nullable * _Nullable)error;
+ (nullable NSNumber *)voiceProcessingEnabledForEngine:(AVAudioEngine *)engine error:(NSError * _Nullable * _Nullable)error;
+ (BOOL)setVoiceProcessing:(BOOL)enabled forEngine:(AVAudioEngine *)engine error:(NSError * _Nullable * _Nullable)error;
+ (nullable AVAudioFormat *)outputFormatForEngine:(AVAudioEngine *)engine bus:(AVAudioNodeBus)bus error:(NSError * _Nullable * _Nullable)error;
+ (BOOL)installTapOnEngine:(AVAudioEngine *)engine
                       bus:(AVAudioNodeBus)bus
                bufferSize:(AVAudioFrameCount)bufferSize
                    format:(nullable AVAudioFormat *)format
                     block:(AVAudioNodeTapBlock)block
                     error:(NSError * _Nullable * _Nullable)error;
+ (BOOL)removeTapOnEngine:(AVAudioEngine *)engine bus:(AVAudioNodeBus)bus error:(NSError * _Nullable * _Nullable)error;
+ (BOOL)attachAndConnectMixer:(AVAudioMixerNode *)mixer
                     onEngine:(AVAudioEngine *)engine
                        input:(AVAudioInputNode *)input
                       format:(AVAudioFormat *)format
                        error:(NSError * _Nullable * _Nullable)error;
+ (BOOL)prepareAndStartEngine:(AVAudioEngine *)engine error:(NSError * _Nullable * _Nullable)error;
+ (void)stopEngine:(AVAudioEngine *)engine;
+ (BOOL)catchObjCException:(NS_NOESCAPE void (^)(void))block error:(NSError * _Nullable * _Nullable)error;

@end

NS_ASSUME_NONNULL_END
