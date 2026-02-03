//
//  ActrBridge.m
//  Actr React Native Bridge
//
//  This file provides the Objective-C bridge macros to expose the Swift ActrBridge
//  class to React Native's JavaScript runtime.
//

#import <React/RCTBridgeModule.h>
#import <React/RCTEventEmitter.h>

@interface RCT_EXTERN_MODULE(ActrBridge, RCTEventEmitter)

/// Start voice stream (was sendVoiceStream)
RCT_EXTERN_METHOD(startVoice:(NSString *)json
                  resolve:(RCTPromiseResolveBlock)resolve
                  reject:(RCTPromiseRejectBlock)reject)

/// Send voice chunk
RCT_EXTERN_METHOD(sendVoiceChunk:(NSString *)json
                  audioData:(NSString *)audioData
                  resolve:(RCTPromiseResolveBlock)resolve
                  reject:(RCTPromiseRejectBlock)reject)

/// End voice stream
RCT_EXTERN_METHOD(endVoice:(NSString *)json
                  resolve:(RCTPromiseResolveBlock)resolve
                  reject:(RCTPromiseRejectBlock)reject)
/// Upload attachment
RCT_EXTERN_METHOD(uploadAttachment:(NSString *)json
                  fileData:(NSString *)fileData
                  resolve:(RCTPromiseResolveBlock)resolve
                  reject:(RCTPromiseRejectBlock)reject)
// MARK: - ActrService Methods

/// Initialize ActrService
RCT_EXTERN_METHOD(initializeService:(NSString *)path
                  resolve:(RCTPromiseResolveBlock)resolve
                  reject:(RCTPromiseRejectBlock)reject)

/// Shutdown ActrService
RCT_EXTERN_METHOD(shutdownService:(RCTPromiseResolveBlock)resolve
                  reject:(RCTPromiseRejectBlock)reject)

/// Send echo message through ActrService
RCT_EXTERN_METHOD(sendTextMessage:(NSString *)message
                  resolve:(RCTPromiseResolveBlock)resolve
                  reject:(RCTPromiseRejectBlock)reject)

/// Get ActrService connection status
RCT_EXTERN_METHOD(getServiceStatus:(RCTPromiseResolveBlock)resolve
                  reject:(RCTPromiseRejectBlock)reject)

@end
