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

// MARK: - ActrSystem Methods

/// Create an ActrSystem from a TOML config file path
RCT_EXTERN_METHOD(createSystemFromPath:(NSString *)configPath
                  resolve:(RCTPromiseResolveBlock)resolve
                  reject:(RCTPromiseRejectBlock)reject)

/// Create an ActrSystem from a TOML config file URL
RCT_EXTERN_METHOD(createSystemFromUrl:(NSString *)configUrl
                  resolve:(RCTPromiseResolveBlock)resolve
                  reject:(RCTPromiseRejectBlock)reject)

/// Spawn a node with a workload
RCT_EXTERN_METHOD(spawnNode:(NSString *)systemId
                  workloadId:(NSString *)workloadId
                  resolve:(RCTPromiseResolveBlock)resolve
                  reject:(RCTPromiseRejectBlock)reject)

// MARK: - ActrNode Methods

/// Start a node and get an ActrRef
RCT_EXTERN_METHOD(startNode:(NSString *)nodeId
                  resolve:(RCTPromiseResolveBlock)resolve
                  reject:(RCTPromiseRejectBlock)reject)

// MARK: - ActrRef Methods

/// Get the actor ID of an ActrRef
RCT_EXTERN_METHOD(getActorId:(NSString *)refId
                  resolve:(RCTPromiseResolveBlock)resolve
                  reject:(RCTPromiseRejectBlock)reject)

/// Perform an RPC call
RCT_EXTERN_METHOD(call:(NSString *)refId
                  routeKey:(NSString *)routeKey
                  requestData:(NSString *)requestData
                  payloadType:(int32_t)payloadType
                  timeoutMs:(int64_t)timeoutMs
                  resolve:(RCTPromiseResolveBlock)resolve
                  reject:(RCTPromiseRejectBlock)reject)

/// Discover actors of a specific type
RCT_EXTERN_METHOD(discover:(NSString *)refId
                  manufacturer:(NSString *)manufacturer
                  name:(NSString *)name
                  limit:(NSInteger)limit
                  resolve:(RCTPromiseResolveBlock)resolve
                  reject:(RCTPromiseRejectBlock)reject)

/// Stop an actor reference
RCT_EXTERN_METHOD(stopActor:(NSString *)refId
                  resolve:(RCTPromiseResolveBlock)resolve
                  reject:(RCTPromiseRejectBlock)reject)

// MARK: - Cleanup Methods

/// Remove a system from storage
RCT_EXTERN_METHOD(removeSystem:(NSString *)systemId
                  resolve:(RCTPromiseResolveBlock)resolve
                  reject:(RCTPromiseRejectBlock)reject)

/// Remove a node from storage
RCT_EXTERN_METHOD(removeNode:(NSString *)nodeId
                  resolve:(RCTPromiseResolveBlock)resolve
                  reject:(RCTPromiseRejectBlock)reject)

// MARK: - ActrService Methods

/// Initialize ActrService
RCT_EXTERN_METHOD(initializeService:(RCTPromiseResolveBlock)resolve
                  reject:(RCTPromiseRejectBlock)reject)

/// Shutdown ActrService
RCT_EXTERN_METHOD(shutdownService:(RCTPromiseResolveBlock)resolve
                  reject:(RCTPromiseRejectBlock)reject)

/// Send echo message through ActrService
RCT_EXTERN_METHOD(sendEcho:(NSString *)message
                  resolve:(RCTPromiseResolveBlock)resolve
                  reject:(RCTPromiseRejectBlock)reject)

/// Get ActrService connection status
RCT_EXTERN_METHOD(getServiceStatus:(RCTPromiseResolveBlock)resolve
                  reject:(RCTPromiseRejectBlock)reject)

@end
