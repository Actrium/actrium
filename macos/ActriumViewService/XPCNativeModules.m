//
//  XPCNativeModules.m
//  ActriumViewService
//
//  Objective-C bridge for native modules in XPC service.
//  Uses RCT_EXTERN_MODULE to expose Swift modules to React Native.
//

#import <React/RCTBridgeModule.h>
#import <objc/message.h>

// MARK: - RCTHost Event Emitter Helper (Bridgeless Mode)

/// Helper function to call RCTHost's callFunctionOnJSModule:method:args:
/// Swift's performSelector only supports up to 2 arguments, so we use this ObjC helper.
/// @param host The RCTHost instance (obtained from rootViewFactory.reactHost)
/// @param module The JS module name (e.g., "RCTDeviceEventEmitter")
/// @param method The method name (e.g., "emit")
/// @param args The arguments array
void RCTHostCallFunction(id host, NSString *module, NSString *method, NSArray *args) {
    if (!host) return;
    SEL sel = NSSelectorFromString(@"callFunctionOnJSModule:method:args:");
    if ([host respondsToSelector:sel]) {
        typedef void (*MsgSendFunc)(id, SEL, NSString *, NSString *, NSArray *);
        MsgSendFunc func = (MsgSendFunc)objc_msgSend;
        func(host, sel, module, method, args);
    }
}

// MARK: - Capability Permission

@interface RCT_EXTERN_MODULE(DocumentSelection, NSObject)
RCT_EXTERN_METHOD(setAvailable:(NSString *)tabId active:(BOOL)active)
@end

@interface RCT_EXTERN_MODULE(RealmService, NSObject)

RCT_EXTERN_METHOD(call:(NSString *)tabId
                  route:(NSString *)route
                  payloadBase64:(NSString *)payloadBase64
                  timeoutMs:(NSNumber *)timeoutMs
                  resolver:(RCTPromiseResolveBlock)resolve
                  rejecter:(RCTPromiseRejectBlock)reject)

RCT_EXTERN_METHOD(subscribeServiceEvents:(NSString *)tabId
                  events:(NSArray<NSString *> *)events
                  resolver:(RCTPromiseResolveBlock)resolve
                  rejecter:(RCTPromiseRejectBlock)reject)

RCT_EXTERN_METHOD(unsubscribeServiceEvents:(NSString *)tabId
                  subscriptionId:(NSString *)subscriptionId)
RCT_EXTERN_METHOD(subscribeServiceStream:(NSString *)tabId
                  streamId:(NSString *)streamId
                  resolver:(RCTPromiseResolveBlock)resolve
                  rejecter:(RCTPromiseRejectBlock)reject)
RCT_EXTERN_METHOD(sendServiceStreamChunk:(NSString *)tabId
                  route:(NSString *)route
                  streamId:(NSString *)streamId
                  sequence:(NSString *)sequence
                  payloadBase64:(NSString *)payloadBase64
                  resolver:(RCTPromiseResolveBlock)resolve
                  rejecter:(RCTPromiseRejectBlock)reject)
RCT_EXTERN_METHOD(prepareAttachmentUpload:(NSString *)tabId
                  request:(NSDictionary *)request
                  resolver:(RCTPromiseResolveBlock)resolve
                  rejecter:(RCTPromiseRejectBlock)reject)

RCT_EXTERN_METHOD(sendAttachmentChunk:(NSString *)tabId
                  request:(NSDictionary *)request
                  resolver:(RCTPromiseResolveBlock)resolve
                  rejecter:(RCTPromiseRejectBlock)reject)

RCT_EXTERN_METHOD(unregisterStream:(NSString *)tabId
                  streamId:(NSString *)streamId
                  resolver:(RCTPromiseResolveBlock)resolve
                  rejecter:(RCTPromiseRejectBlock)reject)

RCT_EXTERN_METHOD(attachmentUploadAvailable:(NSString *)tabId
                  resolver:(RCTPromiseResolveBlock)resolve
                  rejecter:(RCTPromiseRejectBlock)reject)

@end

@interface RCT_EXTERN_MODULE(CapabilityPermission, NSObject)

RCT_EXTERN_METHOD(requestGrant:(NSString *)tabId capability:(NSString *)capability)

@end

@interface RCT_EXTERN_MODULE(TabTitle, NSObject)

RCT_EXTERN_METHOD(setTitle:(NSString *)title
                  tabId:(NSString *)tabId
                  reloadKey:(NSNumber *)reloadKey)

@end

@interface RCT_EXTERN_MODULE(GuestLocalStorage, NSObject)

RCT_EXTERN_METHOD(perform:(NSString *)tabId
                  documentId:(NSString *)documentId
                  operation:(NSString *)operation
                  inputJSON:(NSString *)inputJSON
                  resolver:(RCTPromiseResolveBlock)resolve
                  rejecter:(RCTPromiseRejectBlock)reject)

@end

@interface RCT_EXTERN_MODULE(GuestMenu, NSObject)
RCT_EXTERN_METHOD(show:(NSString *)tabId
                  reloadKey:(NSNumber *)reloadKey
                  contentJSON:(NSString *)contentJSON
                  resolver:(RCTPromiseResolveBlock)resolve
                  rejecter:(RCTPromiseRejectBlock)reject)
@end

@interface RCT_EXTERN_MODULE(GuestFileImport, NSObject)

RCT_EXTERN_METHOD(pick:(NSString *)tabId
                  reloadKey:(NSNumber *)reloadKey
                  source:(NSString *)source
                  accept:(NSArray *)accept
                  multiple:(BOOL)multiple
                  resolver:(RCTPromiseResolveBlock)resolve
                  rejecter:(RCTPromiseRejectBlock)reject)

RCT_EXTERN_METHOD(pickCached:(NSString *)tabId
                  reloadKey:(NSNumber *)reloadKey
                  source:(NSString *)source
                  accept:(NSArray *)accept
                  multiple:(BOOL)multiple
                  resolver:(RCTPromiseResolveBlock)resolve
                  rejecter:(RCTPromiseRejectBlock)reject)

RCT_EXTERN_METHOD(cachedAttachment:(NSString *)tabId
                  reloadKey:(NSNumber *)reloadKey
                  action:(NSString *)action
                  fileId:(NSString *)fileId
                  resolver:(RCTPromiseResolveBlock)resolve
                  rejecter:(RCTPromiseRejectBlock)reject)

@end

@interface RCT_EXTERN_MODULE(GuestTTS, NSObject)

RCT_EXTERN_METHOD(speak:(NSString *)sessionId
                  text:(NSString *)text
                  locale:(NSString *)locale
                  voice:(NSString *)voice
                  resolver:(RCTPromiseResolveBlock)resolve
                  rejecter:(RCTPromiseRejectBlock)reject)

RCT_EXTERN_METHOD(stop:(NSString *)sessionId
                  resolver:(RCTPromiseResolveBlock)resolve
                  rejecter:(RCTPromiseRejectBlock)reject)

@end

// MARK: - StartPageModule

@interface RCT_EXTERN_MODULE(StartPageModule, NSObject)

RCT_EXTERN_METHOD(openURL:(NSString *)urlString)

@end

// MARK: - HistoryModule

@interface RCT_EXTERN_MODULE(HistoryModule, NSObject)

RCT_EXTERN_METHOD(openHistoryItem:(NSString *)tabId :(NSString *)itemId)
RCT_EXTERN_METHOD(deleteHistoryItem:(NSString *)tabId :(NSString *)itemId)
RCT_EXTERN_METHOD(restoreGroup:(NSString *)tabId :(NSString *)groupId)
RCT_EXTERN_METHOD(clearHistory:(NSString *)tabId)
RCT_EXTERN_METHOD(setHistoryPage:(NSString *)tabId :(NSNumber *)page)

@end

// MARK: - BookmarksModule

@interface RCT_EXTERN_MODULE(BookmarksModule, NSObject)

RCT_EXTERN_METHOD(openBookmark:(NSString *)tabId :(NSString *)bookmarkId)
RCT_EXTERN_METHOD(deleteBookmark:(NSString *)tabId :(NSString *)bookmarkId)

@end

// MARK: - NativeLogger

@interface RCT_EXTERN_MODULE(NativeLogger, NSObject)

RCT_EXTERN_METHOD(log:(NSString *)message)
RCT_EXTERN_METHOD(warn:(NSString *)message)
RCT_EXTERN_METHOD(error:(NSString *)message)

@end
