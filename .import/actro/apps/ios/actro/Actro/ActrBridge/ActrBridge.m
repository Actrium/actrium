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
RCT_EXTERN_METHOD(startVoice:(NSDictionary *)params
                  resolve:(RCTPromiseResolveBlock)resolve
                  reject:(RCTPromiseRejectBlock)reject)

/// Send voice chunk
RCT_EXTERN_METHOD(sendVoiceChunk:(NSDictionary *)params
                  audioData:(NSString *)audioData
                  resolve:(RCTPromiseResolveBlock)resolve
                  reject:(RCTPromiseRejectBlock)reject)

/// Prepare attachment upload
RCT_EXTERN_METHOD(prepareAttachmentUpload:(NSDictionary *)params
                  resolve:(RCTPromiseResolveBlock)resolve
                  reject:(RCTPromiseRejectBlock)reject)

/// Send attachment chunk
RCT_EXTERN_METHOD(sendAttachmentChunk:(NSDictionary *)params
                  chunkData:(NSString *)chunkData
                  resolve:(RCTPromiseResolveBlock)resolve
                  reject:(RCTPromiseRejectBlock)reject)
// MARK: - ActrService Methods

/// Initialize ActrService
RCT_EXTERN_METHOD(initializeService:(NSDictionary *)config
                  resolve:(RCTPromiseResolveBlock)resolve
                  reject:(RCTPromiseRejectBlock)reject)

/// Shutdown ActrService
RCT_EXTERN_METHOD(shutdownService:(NSDictionary *)params
                  resolve:(RCTPromiseResolveBlock)resolve
                  reject:(RCTPromiseRejectBlock)reject)

/// Send echo message through ActrService
RCT_EXTERN_METHOD(sendTextMessage:(NSDictionary *)params
                  resolve:(RCTPromiseResolveBlock)resolve
                  reject:(RCTPromiseRejectBlock)reject)

/// Get ActrService connection status
RCT_EXTERN_METHOD(getServiceStatus:(RCTPromiseResolveBlock)resolve
                  reject:(RCTPromiseRejectBlock)reject)

/// Unregister a stream callback
RCT_EXTERN_METHOD(unregisterStream:(NSDictionary *)params
                  resolve:(RCTPromiseResolveBlock)resolve
                  reject:(RCTPromiseRejectBlock)reject)

@end
