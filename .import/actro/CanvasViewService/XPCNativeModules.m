//
//  XPCNativeModules.m
//  CanvasViewService
//
//  Objective-C bridge for native modules in XPC service.
//  Uses RCT_EXTERN_MODULE to expose Swift modules to React Native.
//

#import <React/RCTBridgeModule.h>

// MARK: - StartPageModule

@interface RCT_EXTERN_MODULE(StartPageModule, NSObject)

RCT_EXTERN_METHOD(openURL:(NSString *)urlString)

@end

// MARK: - HistoryModule

@interface RCT_EXTERN_MODULE(HistoryModule, NSObject)

RCT_EXTERN_METHOD(openHistoryItem:(NSString *)itemId :(NSString *)appURL)
RCT_EXTERN_METHOD(deleteHistoryItem:(NSString *)itemId)
RCT_EXTERN_METHOD(restoreGroup:(NSString *)groupId)
RCT_EXTERN_METHOD(clearHistory)

@end

// MARK: - BookmarksModule

@interface RCT_EXTERN_MODULE(BookmarksModule, NSObject)

RCT_EXTERN_METHOD(openBookmark:(NSString *)bookmarkId :(NSString *)appURL)
RCT_EXTERN_METHOD(deleteBookmark:(NSString *)bookmarkId)

@end

// MARK: - NativeLogger

@interface RCT_EXTERN_MODULE(NativeLogger, NSObject)

RCT_EXTERN_METHOD(log:(NSString *)message)
RCT_EXTERN_METHOD(warn:(NSString *)message)
RCT_EXTERN_METHOD(error:(NSString *)message)

@end
