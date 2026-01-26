#import "JSCSandboxJSI.h"
#import <Foundation/Foundation.h>
#import <JavaScriptCore/JavaScriptCore.h>

namespace jsc_sandbox {

// Safe extraction of error message from a JSValue without triggering
// toString recursion. [JSValue toString] executes JS code which can throw,
// re-entering the exception handler and causing stack overflow (SIGBUS).
static NSString *safeExceptionMessage(JSValue *exception) {
  if (!exception) return @"(null exception)";
  if ([exception isString]) {
    return [exception toString];
  }
  // For Error objects, .message is typically a plain string
  JSValue *msg = exception[@"message"];
  if (msg && [msg isString]) {
    JSValue *name = exception[@"name"];
    NSString *nameStr = (name && [name isString]) ? [name toString] : @"Error";
    return [NSString stringWithFormat:@"%@: %@", nameStr, [msg toString]];
  }
  return @"(exception: cannot safely stringify)";
}

// MARK: - JSCSandboxContext Implementation

JSCSandboxContext::JSCSandboxContext(jsi::Runtime &hostRuntime, double timeout)
    : jsContext_(nullptr), hostRuntime_(&hostRuntime), disposed_(false),
      callbackCounter_(0) {
  (void)timeout; // Reserved for future use
  @autoreleasepool {
    JSContext *ctx = [[JSContext alloc] init];
    if (!ctx) {
      throw jsi::JSError(hostRuntime, "Failed to create JSContext");
    }

    // Set up exception handler - must store exception for later checking.
    // CRITICAL: Use recursion guard to prevent infinite toString recursion.
    // [JSValue toString] executes JS code which can throw, re-entering this
    // handler and causing stack overflow (EXC_BAD_ACCESS / SIGBUS).
    __block BOOL inExceptionHandler = NO;
    ctx.exceptionHandler = ^(JSContext *context, JSValue *exception) {
      context.exception = exception; // Preserve for checking after eval
      if (inExceptionHandler) return; // Break recursion cycle
      inExceptionHandler = YES;
      NSLog(@"[JSCSandbox] Exception: %@", safeExceptionMessage(exception));
      inExceptionHandler = NO;
    };

    // Inject console shim
    // message is always a string from JS-side .join(' '), safe to toString.
    // Guard against non-string values to avoid toString recursion.
    JSValue *consoleLog = [JSValue
        valueWithObject:^(JSValue *message) {
          if ([message isString]) {
            NSLog(@"[JSCSandbox] %@", [message toString]);
          } else {
            NSLog(@"[JSCSandbox] [non-string value]");
          }
        }
              inContext:ctx];
    ctx[@"__jsc_console_log"] = consoleLog;

    NSString *consoleScript = @R"(
            var console = {
                log: function() { __jsc_console_log(Array.prototype.slice.call(arguments).join(' ')); },
                warn: function() { __jsc_console_log('[WARN] ' + Array.prototype.slice.call(arguments).join(' ')); },
                error: function() { __jsc_console_log('[ERROR] ' + Array.prototype.slice.call(arguments).join(' ')); },
                info: function() { __jsc_console_log('[INFO] ' + Array.prototype.slice.call(arguments).join(' ')); },
                debug: function() { __jsc_console_log('[DEBUG] ' + Array.prototype.slice.call(arguments).join(' ')); },
                assert: function(cond) { if (!cond) __jsc_console_log('[ASSERT] ' + Array.prototype.slice.call(arguments, 1).join(' ')); },
                trace: function() {},
                time: function() {},
                timeEnd: function() {},
                group: function() {},
                groupEnd: function() {}
            };
        )";
    [ctx evaluateScript:consoleScript];

    jsContext_ = (__bridge_retained void *)ctx;
  }
}

JSCSandboxContext::~JSCSandboxContext() { dispose(); }

void JSCSandboxContext::dispose() {
  std::lock_guard<std::recursive_mutex> lock(mutex_);
  if (disposed_)
    return;
  disposed_ = true;

  if (jsContext_) {
    @autoreleasepool {
      // Transfer ownership to ARC and let it release
      (void)(__bridge_transfer JSContext *)jsContext_;
    }
    jsContext_ = nullptr;
  }
  callbacks_.clear();
}

jsi::Value JSCSandboxContext::get(jsi::Runtime &rt,
                                  const jsi::PropNameID &name) {
  std::string propName = name.utf8(rt);

  if (propName == "eval") {
    return jsi::Function::createFromHostFunction(
        rt, name,
        1, // argc
        [this](jsi::Runtime &rt, const jsi::Value &thisVal,
               const jsi::Value *args, size_t count) -> jsi::Value {
          (void)thisVal;
          if (count < 1 || !args[0].isString()) {
            throw jsi::JSError(rt, "eval requires a string argument");
          }
          std::string code = args[0].asString(rt).utf8(rt);
          return this->eval(rt, code);
        });
  }

  if (propName == "setGlobal") {
    return jsi::Function::createFromHostFunction(
        rt, name,
        2, // argc
        [this](jsi::Runtime &rt, const jsi::Value &thisVal,
               const jsi::Value *args, size_t count) -> jsi::Value {
          (void)thisVal;
          if (count < 2 || !args[0].isString()) {
            throw jsi::JSError(rt,
                               "setGlobal requires (name: string, value: any)");
          }
          std::string globalName = args[0].asString(rt).utf8(rt);
          this->setGlobal(rt, globalName, args[1]);
          return jsi::Value::undefined();
        });
  }

  if (propName == "getGlobal") {
    return jsi::Function::createFromHostFunction(
        rt, name,
        1, // argc
        [this](jsi::Runtime &rt, const jsi::Value &thisVal,
               const jsi::Value *args, size_t count) -> jsi::Value {
          (void)thisVal;
          if (count < 1 || !args[0].isString()) {
            throw jsi::JSError(rt, "getGlobal requires a string argument");
          }
          std::string globalName = args[0].asString(rt).utf8(rt);
          return this->getGlobal(rt, globalName);
        });
  }

  if (propName == "dispose") {
    return jsi::Function::createFromHostFunction(
        rt, name, 0,
        [this](jsi::Runtime &rt, const jsi::Value &thisVal,
               const jsi::Value *args, size_t count) -> jsi::Value {
          (void)rt;
          (void)thisVal;
          (void)args;
          (void)count;
          this->dispose();
          return jsi::Value::undefined();
        });
  }

  if (propName == "isDisposed") {
    return jsi::Value(disposed_);
  }

  return jsi::Value::undefined();
}

void JSCSandboxContext::set(jsi::Runtime &rt, const jsi::PropNameID &name,
                            const jsi::Value &value) {
  (void)rt;
  (void)name;
  (void)value;
  // Read-only
}

std::vector<jsi::PropNameID>
JSCSandboxContext::getPropertyNames(jsi::Runtime &rt) {
  std::vector<jsi::PropNameID> props;
  props.push_back(jsi::PropNameID::forUtf8(rt, "eval"));
  props.push_back(jsi::PropNameID::forUtf8(rt, "setGlobal"));
  props.push_back(jsi::PropNameID::forUtf8(rt, "getGlobal"));
  props.push_back(jsi::PropNameID::forUtf8(rt, "dispose"));
  props.push_back(jsi::PropNameID::forUtf8(rt, "isDisposed"));
  return props;
}

jsi::Value JSCSandboxContext::eval(jsi::Runtime &rt, const std::string &code) {
  std::lock_guard<std::recursive_mutex> lock(mutex_);

  if (disposed_) {
    throw jsi::JSError(rt, "Context has been disposed");
  }

  @autoreleasepool {
    JSContext *ctx = (__bridge JSContext *)jsContext_;
    NSString *nsCode = [NSString stringWithUTF8String:code.c_str()];

    JSValue *result = [ctx evaluateScript:nsCode];

    // Check for exceptions
    if (ctx.exception) {
      NSString *errorMsg = safeExceptionMessage(ctx.exception);
      ctx.exception = nil;
      throw jsi::JSError(rt, [errorMsg UTF8String]);
    }

    return jsValueToJSI(rt, (__bridge void *)result);
  }
}

void JSCSandboxContext::setGlobal(jsi::Runtime &rt, const std::string &name,
                                  const jsi::Value &value) {
  std::lock_guard<std::recursive_mutex> lock(mutex_);

  if (disposed_) {
    throw jsi::JSError(rt, "Context has been disposed");
  }

  @autoreleasepool {
    JSContext *ctx = (__bridge JSContext *)jsContext_;
    NSString *nsName = [NSString stringWithUTF8String:name.c_str()];

    // Handle functions specially - create a wrapper function in JS that calls
    // our block This ensures typeof returns "function" instead of "object"
    if (value.isObject() && value.asObject(rt).isFunction(rt)) {
      // Store the function
      std::string callbackId = "cb_" + std::to_string(++callbackCounter_);
      auto func =
          std::make_shared<jsi::Function>(value.asObject(rt).asFunction(rt));
      callbacks_[callbackId] = func;

      // Capture what we need for the block
      jsi::Runtime *hostRt = hostRuntime_;
      std::string cbId = callbackId;
      auto *self = this;

      // First, create a block-based function with internal name
      NSString *internalName =
          [NSString stringWithFormat:@"__jsc_cb_%s", cbId.c_str()];
      JSValue *blockFn = [JSValue
          valueWithObject:^id(void) {
            NSArray *jsArgs = [JSContext currentArguments];

            @autoreleasepool {
              auto it = self->callbacks_.find(cbId);
              if (it == self->callbacks_.end()) {
                NSLog(@"[JSCSandbox] Callback not found: %s", cbId.c_str());
                return nil;
              }

              try {
                std::vector<jsi::Value> jsiArgs;
                for (NSUInteger i = 0; i < jsArgs.count; i++) {
                  JSValue *arg = jsArgs[i];
                  jsiArgs.push_back(
                      self->jsValueToJSI(*hostRt, (__bridge void *)arg));
                }

                jsi::Value result;
                if (jsiArgs.empty()) {
                  result = it->second->call(*hostRt);
                } else {
                  result = it->second->call(*hostRt,
                                            (const jsi::Value *)jsiArgs.data(),
                                            jsiArgs.size());
                }

                void *jsResult = self->jsiToJSValue(*hostRt, result);
                return (__bridge id)jsResult;
              } catch (const std::exception &e) {
                NSLog(@"[JSCSandbox] Callback %s exception: %s", cbId.c_str(),
                      e.what());
                return nil;
              }
            }
          }
                inContext:ctx];

      // Store the block function with internal name
      ctx[internalName] = blockFn;

      // Create a proper function wrapper using eval
      // This ensures typeof returns "function"
      NSString *wrapperScript =
          [NSString stringWithFormat:@"(function() { return function(...args) "
                                     @"{ return %@.apply(this, args); }; })()",
                                     internalName];
      JSValue *wrapperFn = [ctx evaluateScript:wrapperScript];

      // Set the wrapper as the global with the requested name
      ctx[nsName] = wrapperFn;

      // Also set on globalThis - use internalName which is accessible as a
      // global variable
      NSString *globalThisScript = [NSString
          stringWithFormat:
              @"(function() { var fn = %@; globalThis['%@'] = function() { "
              @"return fn.apply(this, arguments); }; })()",
              internalName, nsName];
      [ctx evaluateScript:globalThisScript];
    } else {
      // Convert and set non-function values
      void *jsValue = jsiToJSValue(rt, value);
      JSValue *jsVal = (__bridge JSValue *)jsValue;

      // Use a temp name to store value, then assign to both ctx and globalThis
      static int nonFuncCounter = 0;
      NSString *tempName =
          [NSString stringWithFormat:@"__jsc_tmp_%d__", ++nonFuncCounter];
      ctx[tempName] = jsVal;
      ctx[nsName] = jsVal;

      // Set on globalThis using temp name which is accessible
      NSString *globalThisScript = [NSString
          stringWithFormat:@"(function() { globalThis['%@'] = %@; })()", nsName,
                           tempName];
      [ctx evaluateScript:globalThisScript];
    }
  }
}

jsi::Value JSCSandboxContext::getGlobal(jsi::Runtime &rt,
                                        const std::string &name) {
  std::lock_guard<std::recursive_mutex> lock(mutex_);

  if (disposed_) {
    throw jsi::JSError(rt, "Context has been disposed");
  }

  @autoreleasepool {
    JSContext *ctx = (__bridge JSContext *)jsContext_;
    NSString *nsName = [NSString stringWithUTF8String:name.c_str()];
    JSValue *value = ctx[nsName];
    return jsValueToJSI(rt, (__bridge void *)value);
  }
}

// Helper: wrap a jsi::Function into a JSValue function callable in the sandbox
void *JSCSandboxContext::wrapFunctionForSandbox(jsi::Runtime & /*rt*/,
                                                jsi::Function &&func) {
  JSContext *ctx = (__bridge JSContext *)jsContext_;

  // Generate unique internal name for this function
  static int funcCounter = 0;
  int funcId = ++funcCounter;
  NSString *internalName = [NSString stringWithFormat:@"__jsc_fn_%d__", funcId];

  // Store the function in callbacks_ with the internal name
  std::string internalNameStr = [internalName UTF8String];
  callbacks_[internalNameStr] =
      std::make_shared<jsi::Function>(std::move(func));

  // Get reference to host runtime
  jsi::Runtime *hostRt = hostRuntime_;
  auto self = this;

  // Create native block that calls the stored function
  JSValue *blockFn = [JSValue
      valueWithObject:^id(void) {
        NSArray *args = [JSContext currentArguments];

        @autoreleasepool {
          std::lock_guard<std::recursive_mutex> lock(self->mutex_);

          auto it = self->callbacks_.find(internalNameStr);
          if (it == self->callbacks_.end()) {
            NSLog(@"[JSCSandbox] Wrapped fn_%d: function not found!", funcId);
            return nil;
          }

          try {
            // Convert JSValue args to jsi::Value
            std::vector<jsi::Value> jsiArgs;
            for (NSUInteger i = 0; i < args.count; i++) {
              JSValue *arg = args[i];
              jsiArgs.push_back(
                  self->jsValueToJSI(*hostRt, (__bridge void *)arg));
            }

            jsi::Value result;
            if (jsiArgs.empty()) {
              result = it->second->call(*hostRt);
            } else {
              result = it->second->call(
                  *hostRt, (const jsi::Value *)jsiArgs.data(), jsiArgs.size());
            }

            void *jsResult = self->jsiToJSValue(*hostRt, result);
            return (__bridge id)jsResult;
          } catch (const std::exception &e) {
            NSLog(@"[JSCSandbox] fn_%d exception: %s", funcId, e.what());
            return nil;
          } catch (...) {
            NSLog(@"[JSCSandbox] fn_%d unknown exception!", funcId);
            return nil;
          }
        }
      }
            inContext:ctx];

  // Store the block function with internal name
  ctx[internalName] = blockFn;

  // Create a proper function wrapper using eval (ensures typeof returns
  // "function")
  NSString *wrapperScript = [NSString
      stringWithFormat:@"(function() { var fn = %@; return function(...args) { "
                       @"return fn(...args); }; })()",
                       internalName];
  JSValue *wrapperFn = [ctx evaluateScript:wrapperScript];

  return (__bridge void *)wrapperFn;
}

// Convert jsi::Value to JSValue*
void *JSCSandboxContext::jsiToJSValue(jsi::Runtime &rt,
                                      const jsi::Value &value) {
  JSContext *ctx = (__bridge JSContext *)jsContext_;

  if (value.isUndefined()) {
    return (__bridge void *)[JSValue valueWithUndefinedInContext:ctx];
  }
  if (value.isNull()) {
    return (__bridge void *)[JSValue valueWithNullInContext:ctx];
  }
  if (value.isBool()) {
    return (__bridge void *)[JSValue valueWithBool:value.getBool()
                                         inContext:ctx];
  }
  if (value.isNumber()) {
    return (__bridge void *)[JSValue valueWithDouble:value.getNumber()
                                           inContext:ctx];
  }
  if (value.isString()) {
    std::string str = value.asString(rt).utf8(rt);
    NSString *nsStr = [NSString stringWithUTF8String:str.c_str()];
    return (__bridge void *)[JSValue valueWithObject:nsStr inContext:ctx];
  }
  // Handle Symbols
  if (value.isSymbol()) {
    jsi::Symbol sym = value.getSymbol(rt);
    std::string symDesc = sym.toString(rt);
    NSString *nsSymDesc = [NSString stringWithUTF8String:symDesc.c_str()];

    // Try to extract the key from "Symbol(key)" format
    NSRegularExpression *regex =
        [NSRegularExpression regularExpressionWithPattern:@"Symbol\\((.+)\\)"
                                                  options:0
                                                    error:nil];
    NSTextCheckingResult *match =
        [regex firstMatchInString:nsSymDesc
                          options:0
                            range:NSMakeRange(0, nsSymDesc.length)];

    if (match && match.numberOfRanges > 1) {
      NSString *symKey = [nsSymDesc substringWithRange:[match rangeAtIndex:1]];
      // Use Symbol.for() to create a global Symbol with the same key in JSC
      NSString *script =
          [NSString stringWithFormat:@"Symbol.for('%@')", symKey];
      JSValue *jscSymbol = [ctx evaluateScript:script];
      return (__bridge void *)jscSymbol;
    } else {
      // For non-registered Symbols, create a new Symbol with the description
      NSString *script = [NSString stringWithFormat:@"Symbol('%@')", nsSymDesc];
      JSValue *jscSymbol = [ctx evaluateScript:script];
      return (__bridge void *)jscSymbol;
    }
  }
  if (value.isObject()) {
    jsi::Object obj = value.asObject(rt);

    // Handle functions FIRST (before general object handling)
    if (obj.isFunction(rt)) {
      jsi::Function func = obj.asFunction(rt);
      return wrapFunctionForSandbox(rt, std::move(func));
    }

    // Handle arrays
    if (obj.isArray(rt)) {
      jsi::Array arr = obj.asArray(rt);
      size_t len = arr.size(rt);
      NSMutableArray *nsArr = [NSMutableArray arrayWithCapacity:len];
      for (size_t i = 0; i < len; i++) {
        JSValue *elem =
            (__bridge JSValue *)jsiToJSValue(rt, arr.getValueAtIndex(rt, i));
        [nsArr addObject:elem ?: [NSNull null]];
      }
      return (__bridge void *)[JSValue valueWithObject:nsArr inContext:ctx];
    }

    // Handle plain objects
    jsi::Array propNames = obj.getPropertyNames(rt);
    size_t len = propNames.size(rt);

    // Use JavaScript object directly instead of NSDictionary to preserve
    // functions
    JSValue *jsObj = [ctx evaluateScript:@"({})"];

    for (size_t i = 0; i < len; i++) {
      std::string key = propNames.getValueAtIndex(rt, i).asString(rt).utf8(rt);
      NSString *nsKey = [NSString stringWithUTF8String:key.c_str()];
      jsi::Value propVal = obj.getProperty(rt, key.c_str());

      // Convert all values including functions
      JSValue *jsVal = (__bridge JSValue *)jsiToJSValue(rt, propVal);
      if (jsVal) {
        jsObj[nsKey] = jsVal;
      }
    }
    return (__bridge void *)jsObj;
  }

  return (__bridge void *)[JSValue valueWithUndefinedInContext:ctx];
}

// Convert JSValue* to jsi::Value (entry point, delegates to depth-limited version)
jsi::Value JSCSandboxContext::jsValueToJSI(jsi::Runtime &rt, void *jsValue) {
  return jsValueToJSI(rt, jsValue, 0);
}

// Depth-limited conversion to prevent stack overflow from circular references
static constexpr int kMaxConversionDepth = 32;

jsi::Value JSCSandboxContext::jsValueToJSI(jsi::Runtime &rt, void *jsValue,
                                            int depth) {
  JSValue *value = (__bridge JSValue *)jsValue;

  if (!value || [value isUndefined]) {
    return jsi::Value::undefined();
  }
  if ([value isNull]) {
    return jsi::Value::null();
  }
  if ([value isBoolean]) {
    return jsi::Value([value toBool]);
  }
  if ([value isNumber]) {
    return jsi::Value([value toDouble]);
  }
  if ([value isString]) {
    NSString *str = [value toString];
    return jsi::String::createFromUtf8(rt, [str UTF8String]);
  }

  // Depth guard for recursive structures (arrays, objects)
  if (depth >= kMaxConversionDepth) {
    NSLog(@"[JSCSandbox] jsValueToJSI: max depth %d reached, returning "
          @"undefined",
          kMaxConversionDepth);
    return jsi::Value::undefined();
  }

  if ([value isArray]) {
    NSArray *arr = [value toArray];
    jsi::Array jsiArr = jsi::Array(rt, arr.count);
    for (NSUInteger i = 0; i < arr.count; i++) {
      id elem = arr[i];
      if ([elem isKindOfClass:[JSValue class]]) {
        jsiArr.setValueAtIndex(
            rt, i, jsValueToJSI(rt, (__bridge void *)elem, depth + 1));
      } else if ([elem isKindOfClass:[NSNumber class]]) {
        jsiArr.setValueAtIndex(rt, i, jsi::Value([elem doubleValue]));
      } else if ([elem isKindOfClass:[NSString class]]) {
        jsiArr.setValueAtIndex(
            rt, i, jsi::String::createFromUtf8(rt, [elem UTF8String]));
      } else if ([elem isKindOfClass:[NSNull class]]) {
        jsiArr.setValueAtIndex(rt, i, jsi::Value::null());
      } else if ([elem isKindOfClass:[NSDictionary class]]) {
        JSContext *ctx = (__bridge JSContext *)jsContext_;
        JSValue *nestedValue = [JSValue valueWithObject:elem inContext:ctx];
        jsiArr.setValueAtIndex(
            rt, i,
            jsValueToJSI(rt, (__bridge void *)nestedValue, depth + 1));
      }
    }
    return std::move(jsiArr);
  }
  if ([value isObject]) {
    JSContext *ctx = (__bridge JSContext *)jsContext_;

    // Check if the value is a function FIRST
    // Use JavaScript's typeof to accurately detect functions
    JSValue *typeofResult =
        [ctx evaluateScript:@"(function(v) { return typeof v; })"];
    JSValue *typeStr = [typeofResult callWithArguments:@[ value ]];
    NSString *typeString = [typeStr toString];

    if ([typeString isEqualToString:@"function"]) {

      // Store the sandbox function for later invocation
      static int sandboxFuncCounter = 0;
      NSString *funcKey = [NSString
          stringWithFormat:@"__sandbox_fn_%d__", ++sandboxFuncCounter];
      ctx[funcKey] = value;

      // Capture what we need for the proxy
      std::string funcKeyStr = [funcKey UTF8String];
      auto *self = this;

      // Create a JSI host function that proxies calls to the sandbox function
      return jsi::Function::createFromHostFunction(
          rt, jsi::PropNameID::forUtf8(rt, "sandboxProxy"),
          0, // variadic
          [self, funcKeyStr](jsi::Runtime &rt, const jsi::Value &thisVal,
                             const jsi::Value *args,
                             size_t count) -> jsi::Value {
            (void)thisVal;
            @autoreleasepool {
              std::lock_guard<std::recursive_mutex> lock(self->mutex_);

              if (self->disposed_) {
                throw jsi::JSError(rt, "Context has been disposed");
              }

              JSContext *ctx = (__bridge JSContext *)self->jsContext_;
              NSString *funcKey =
                  [NSString stringWithUTF8String:funcKeyStr.c_str()];
              JSValue *sandboxFunc = ctx[funcKey];

              if (!sandboxFunc || [sandboxFunc isUndefined]) {
                throw jsi::JSError(rt, "Sandbox function not found");
              }

              // Convert args to JSValue array
              NSMutableArray *jsArgs = [NSMutableArray arrayWithCapacity:count];
              for (size_t i = 0; i < count; i++) {
                JSValue *jsArg =
                    (__bridge JSValue *)self->jsiToJSValue(rt, args[i]);
                [jsArgs
                    addObject:jsArg
                                  ?: [JSValue valueWithUndefinedInContext:ctx]];
              }

              // Call the sandbox function
              JSValue *result = [sandboxFunc callWithArguments:jsArgs];

              // Check for exceptions
              if (ctx.exception) {
                NSString *errorMsg = safeExceptionMessage(ctx.exception);
                ctx.exception = nil;
                throw jsi::JSError(rt, [errorMsg UTF8String]);
              }

              return self->jsValueToJSI(rt, (__bridge void *)result);
            }
          });
    }

    // Not a function, convert as regular object
    jsi::Object jsiObj = jsi::Object(rt);

    // Get all own property names using JavaScript
    JSValue *getKeysFunc =
        [ctx evaluateScript:@"(function(obj) { return Object.keys(obj); })"];
    JSValue *keysArray = [getKeysFunc callWithArguments:@[ value ]];

    if (keysArray && [keysArray isArray]) {
      NSArray *keys = [keysArray toArray];

      for (NSString *key in keys) {
        if (![key isKindOfClass:[NSString class]])
          continue;

        JSValue *propVal = value[key];
        if (!propVal || [propVal isUndefined])
          continue;

        // Recursively convert each property with depth tracking
        jsi::Value jsiPropVal =
            jsValueToJSI(rt, (__bridge void *)propVal, depth + 1);
        jsiObj.setProperty(rt, [key UTF8String], std::move(jsiPropVal));
      }
    }

    return std::move(jsiObj);
  }

  return jsi::Value::undefined();
}

// MARK: - JSCSandboxRuntime Implementation

JSCSandboxRuntime::JSCSandboxRuntime(jsi::Runtime &hostRuntime, double timeout)
    : hostRuntime_(&hostRuntime), timeout_(timeout), disposed_(false) {}

JSCSandboxRuntime::~JSCSandboxRuntime() { dispose(); }

void JSCSandboxRuntime::dispose() {
  std::lock_guard<std::recursive_mutex> lock(mutex_);
  if (disposed_)
    return;
  disposed_ = true;

  for (auto &ctx : contexts_) {
    ctx->dispose();
  }
  contexts_.clear();
}

jsi::Value JSCSandboxRuntime::get(jsi::Runtime &rt,
                                  const jsi::PropNameID &name) {
  std::string propName = name.utf8(rt);

  if (propName == "createContext") {
    return jsi::Function::createFromHostFunction(
        rt, name, 0,
        [this](jsi::Runtime &rt, const jsi::Value &thisVal,
               const jsi::Value *args, size_t count) -> jsi::Value {
          (void)thisVal;
          (void)args;
          (void)count;
          return this->createContext(rt);
        });
  }

  if (propName == "dispose") {
    return jsi::Function::createFromHostFunction(
        rt, name, 0,
        [this](jsi::Runtime &rt, const jsi::Value &thisVal,
               const jsi::Value *args, size_t count) -> jsi::Value {
          (void)rt;
          (void)thisVal;
          (void)args;
          (void)count;
          this->dispose();
          return jsi::Value::undefined();
        });
  }

  return jsi::Value::undefined();
}

void JSCSandboxRuntime::set(jsi::Runtime &rt, const jsi::PropNameID &name,
                            const jsi::Value &value) {
  (void)rt;
  (void)name;
  (void)value;
  // Read-only
}

std::vector<jsi::PropNameID>
JSCSandboxRuntime::getPropertyNames(jsi::Runtime &rt) {
  std::vector<jsi::PropNameID> props;
  props.push_back(jsi::PropNameID::forUtf8(rt, "createContext"));
  props.push_back(jsi::PropNameID::forUtf8(rt, "dispose"));
  return props;
}

jsi::Value JSCSandboxRuntime::createContext(jsi::Runtime &rt) {
  std::lock_guard<std::recursive_mutex> lock(mutex_);

  if (disposed_) {
    throw jsi::JSError(rt, "Runtime has been disposed");
  }

  auto context = std::make_shared<JSCSandboxContext>(*hostRuntime_, timeout_);
  contexts_.push_back(context);

  return jsi::Object::createFromHostObject(rt, context);
}

// MARK: - JSCSandboxModule Implementation

JSCSandboxModule::JSCSandboxModule(jsi::Runtime &runtime) {
  (void)runtime; // Module does not need to store runtime reference
}

JSCSandboxModule::~JSCSandboxModule() {}

jsi::Value JSCSandboxModule::get(jsi::Runtime &rt,
                                 const jsi::PropNameID &name) {
  std::string propName = name.utf8(rt);

  if (propName == "createRuntime") {
    return jsi::Function::createFromHostFunction(
        rt, name, 1,
        [](jsi::Runtime &rt, const jsi::Value &thisVal, const jsi::Value *args,
           size_t count) -> jsi::Value {
          (void)thisVal;
          double timeout = 30000; // default 30s

          if (count > 0 && args[0].isObject()) {
            jsi::Object opts = args[0].asObject(rt);
            if (opts.hasProperty(rt, "timeout")) {
              jsi::Value timeoutVal = opts.getProperty(rt, "timeout");
              if (timeoutVal.isNumber()) {
                timeout = timeoutVal.getNumber();
              }
            }
          }

          auto runtime = std::make_shared<JSCSandboxRuntime>(rt, timeout);
          return jsi::Object::createFromHostObject(rt, runtime);
        });
  }

  if (propName == "isAvailable") {
    return jsi::Function::createFromHostFunction(
        rt, name, 0,
        [](jsi::Runtime &rt, const jsi::Value &thisVal, const jsi::Value *args,
           size_t count) -> jsi::Value {
          (void)rt;
          (void)thisVal;
          (void)args;
          (void)count;
          return jsi::Value(true);
        });
  }

  return jsi::Value::undefined();
}

void JSCSandboxModule::set(jsi::Runtime &rt, const jsi::PropNameID &name,
                           const jsi::Value &value) {
  (void)rt;
  (void)name;
  (void)value;
  // Read-only
}

std::vector<jsi::PropNameID>
JSCSandboxModule::getPropertyNames(jsi::Runtime &rt) {
  std::vector<jsi::PropNameID> props;
  props.push_back(jsi::PropNameID::forUtf8(rt, "createRuntime"));
  props.push_back(jsi::PropNameID::forUtf8(rt, "isAvailable"));
  return props;
}

void JSCSandboxModule::install(jsi::Runtime &runtime) {
  auto module = std::make_shared<JSCSandboxModule>(runtime);
  jsi::Object moduleObj = jsi::Object::createFromHostObject(runtime, module);
  runtime.global().setProperty(runtime, "__JSCSandboxJSI",
                               std::move(moduleObj));
}

// Wrapper function for external linkage (avoids JSValue symbol conflicts)
void installJSCSandbox(jsi::Runtime &runtime) {
  JSCSandboxModule::install(runtime);
}

} // namespace jsc_sandbox
