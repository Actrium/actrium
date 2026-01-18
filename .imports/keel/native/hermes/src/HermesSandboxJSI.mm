#import "HermesSandboxJSI.h"
#import <Foundation/Foundation.h>
#import <hermes/hermes.h>

namespace hermes_sandbox {

// MARK: - Value Conversion Helpers

// Deep copy a JSI value from one runtime to another
// This is necessary because JSI values are tied to their runtime
jsi::Value HermesSandboxContext::hostToSandbox(jsi::Runtime &hostRt,
                                                jsi::Runtime &sandboxRt,
                                                const jsi::Value &value) {
  if (value.isUndefined()) {
    return jsi::Value::undefined();
  }
  if (value.isNull()) {
    return jsi::Value::null();
  }
  if (value.isBool()) {
    return jsi::Value(value.getBool());
  }
  if (value.isNumber()) {
    return jsi::Value(value.getNumber());
  }
  if (value.isString()) {
    return jsi::String::createFromUtf8(sandboxRt,
                                       value.getString(hostRt).utf8(hostRt));
  }
  if (value.isSymbol()) {
    // Symbols cannot be transferred between runtimes
    return jsi::Value::undefined();
  }
  if (value.isObject()) {
    jsi::Object obj = value.getObject(hostRt);

    // Handle arrays
    if (obj.isArray(hostRt)) {
      jsi::Array arr = obj.getArray(hostRt);
      size_t length = arr.size(hostRt);
      jsi::Array newArr = jsi::Array(sandboxRt, length);
      for (size_t i = 0; i < length; i++) {
        newArr.setValueAtIndex(
            sandboxRt, i,
            hostToSandbox(hostRt, sandboxRt, arr.getValueAtIndex(hostRt, i)));
      }
      return newArr;
    }

    // Handle functions - wrap as a callback to host
    if (obj.isFunction(hostRt)) {
      return wrapHostFunctionForSandbox(hostRt, sandboxRt, obj.asFunction(hostRt));
    }

    // Handle plain objects
    jsi::Object newObj = jsi::Object(sandboxRt);
    jsi::Array names = obj.getPropertyNames(hostRt);
    size_t length = names.size(hostRt);
    for (size_t i = 0; i < length; i++) {
      jsi::String name = names.getValueAtIndex(hostRt, i).getString(hostRt);
      std::string nameStr = name.utf8(hostRt);
      jsi::Value propValue = obj.getProperty(hostRt, name);
      newObj.setProperty(sandboxRt, nameStr.c_str(),
                         hostToSandbox(hostRt, sandboxRt, propValue));
    }
    return newObj;
  }

  return jsi::Value::undefined();
}

jsi::Value HermesSandboxContext::sandboxToHost(jsi::Runtime &sandboxRt,
                                                jsi::Runtime &hostRt,
                                                const jsi::Value &value) {
  if (value.isUndefined()) {
    return jsi::Value::undefined();
  }
  if (value.isNull()) {
    return jsi::Value::null();
  }
  if (value.isBool()) {
    return jsi::Value(value.getBool());
  }
  if (value.isNumber()) {
    return jsi::Value(value.getNumber());
  }
  if (value.isString()) {
    return jsi::String::createFromUtf8(hostRt,
                                       value.getString(sandboxRt).utf8(sandboxRt));
  }
  if (value.isSymbol()) {
    return jsi::Value::undefined();
  }
  if (value.isObject()) {
    jsi::Object obj = value.getObject(sandboxRt);

    if (obj.isArray(sandboxRt)) {
      jsi::Array arr = obj.getArray(sandboxRt);
      size_t length = arr.size(sandboxRt);
      jsi::Array newArr = jsi::Array(hostRt, length);
      for (size_t i = 0; i < length; i++) {
        newArr.setValueAtIndex(
            hostRt, i,
            sandboxToHost(sandboxRt, hostRt, arr.getValueAtIndex(sandboxRt, i)));
      }
      return newArr;
    }

    if (obj.isFunction(sandboxRt)) {
      return wrapSandboxFunctionForHost(sandboxRt, hostRt, obj.asFunction(sandboxRt));
    }

    jsi::Object newObj = jsi::Object(hostRt);
    jsi::Array names = obj.getPropertyNames(sandboxRt);
    size_t length = names.size(sandboxRt);
    for (size_t i = 0; i < length; i++) {
      jsi::String name = names.getValueAtIndex(sandboxRt, i).getString(sandboxRt);
      std::string nameStr = name.utf8(sandboxRt);
      jsi::Value propValue = obj.getProperty(sandboxRt, name);
      newObj.setProperty(hostRt, nameStr.c_str(),
                         sandboxToHost(sandboxRt, hostRt, propValue));
    }
    return newObj;
  }

  return jsi::Value::undefined();
}

// Wrap a host function for use in sandbox
// Creates a HostFunction in sandbox that proxies calls to the stored host function
jsi::Value HermesSandboxContext::wrapHostFunctionForSandbox(jsi::Runtime & /*hostRt*/,
                                                             jsi::Runtime &sandboxRt,
                                                             jsi::Function &&func) {
  // Store the function with a unique callback ID
  std::string callbackId = "cb_" + std::to_string(++callbackCounter_);
  callbacks_[callbackId] = std::make_shared<jsi::Function>(std::move(func));

  // Capture what we need for the lambda
  std::string cbId = callbackId;
  auto *self = this;

  // Create a HostFunction in sandbox that calls the stored host function
  return jsi::Function::createFromHostFunction(
      sandboxRt,
      jsi::PropNameID::forAscii(sandboxRt, cbId),
      0, // variadic
      [self, cbId](jsi::Runtime &rt, const jsi::Value &thisVal,
                   const jsi::Value *args, size_t count) -> jsi::Value {
        (void)thisVal;

        std::lock_guard<std::recursive_mutex> lock(self->mutex_);

        if (self->disposed_) {
          throw jsi::JSError(rt, "Context has been disposed");
        }

        // Use stored hostRuntime_ pointer
        jsi::Runtime *hostRt = self->hostRuntime_;
        if (!hostRt) {
          throw jsi::JSError(rt, "Host runtime is null");
        }

        auto it = self->callbacks_.find(cbId);
        if (it == self->callbacks_.end()) {
          NSLog(@"[HermesSandbox] Callback not found: %s", cbId.c_str());
          return jsi::Value::undefined();
        }

        try {
          // Convert args from sandbox to host
          std::vector<jsi::Value> hostArgs;
          for (size_t i = 0; i < count; i++) {
            hostArgs.push_back(self->sandboxToHost(rt, *hostRt, args[i]));
          }

          // Call the host function
          jsi::Value result;
          if (hostArgs.empty()) {
            result = it->second->call(*hostRt);
          } else {
            // Use the (Runtime&, const Value*, size_t) overload
            result = it->second->call(
                *hostRt,
                static_cast<const jsi::Value*>(hostArgs.data()),
                hostArgs.size());
          }

          // Convert result from host to sandbox
          return self->hostToSandbox(*hostRt, rt, result);
        } catch (const jsi::JSError &e) {
          NSLog(@"[HermesSandbox] Callback %s JSError: %s", cbId.c_str(), e.what());
          throw jsi::JSError(rt, e.what());
        } catch (const std::exception &e) {
          NSLog(@"[HermesSandbox] Callback %s exception: %s", cbId.c_str(), e.what());
          return jsi::Value::undefined();
        }
      });
}

// Wrap a sandbox function for use in host
// Creates a HostFunction in host that proxies calls to the stored sandbox function
jsi::Value HermesSandboxContext::wrapSandboxFunctionForHost(jsi::Runtime & /*sandboxRt*/,
                                                             jsi::Runtime &hostRt,
                                                             jsi::Function &&func) {
  // Store the sandbox function with a unique ID
  std::string funcId = "sfn_" + std::to_string(++sandboxFunctionCounter_);
  sandboxFunctions_[funcId] = std::make_shared<jsi::Function>(std::move(func));

  // Capture what we need for the lambda
  std::string fId = funcId;
  auto *self = this;

  // Create a HostFunction in host that calls the stored sandbox function
  return jsi::Function::createFromHostFunction(
      hostRt,
      jsi::PropNameID::forAscii(hostRt, fId),
      0, // variadic
      [self, fId](jsi::Runtime &rt, const jsi::Value &thisVal,
                  const jsi::Value *args, size_t count) -> jsi::Value {
        (void)thisVal;

        std::lock_guard<std::recursive_mutex> lock(self->mutex_);

        if (self->disposed_) {
          throw jsi::JSError(rt, "Context has been disposed");
        }

        // Get the sandbox runtime
        jsi::Runtime *sandboxRt = self->sandboxRuntime_.get();
        if (!sandboxRt) {
          throw jsi::JSError(rt, "Sandbox runtime is null");
        }

        auto it = self->sandboxFunctions_.find(fId);
        if (it == self->sandboxFunctions_.end()) {
          NSLog(@"[HermesSandbox] Sandbox function not found: %s", fId.c_str());
          return jsi::Value::undefined();
        }

        try {
          // Convert args from host to sandbox
          std::vector<jsi::Value> sandboxArgs;
          for (size_t i = 0; i < count; i++) {
            sandboxArgs.push_back(self->hostToSandbox(rt, *sandboxRt, args[i]));
          }

          // Call the sandbox function
          jsi::Value result;
          if (sandboxArgs.empty()) {
            result = it->second->call(*sandboxRt);
          } else {
            result = it->second->call(
                *sandboxRt,
                static_cast<const jsi::Value*>(sandboxArgs.data()),
                sandboxArgs.size());
          }

          // Convert result from sandbox to host
          return self->sandboxToHost(*sandboxRt, rt, result);
        } catch (const jsi::JSError &e) {
          NSLog(@"[HermesSandbox] Sandbox function %s JSError: %s", fId.c_str(), e.what());
          throw jsi::JSError(rt, e.what());
        } catch (const std::exception &e) {
          NSLog(@"[HermesSandbox] Sandbox function %s exception: %s", fId.c_str(), e.what());
          return jsi::Value::undefined();
        }
      });
}

// MARK: - HermesSandboxContext Implementation

HermesSandboxContext::HermesSandboxContext(jsi::Runtime &hostRuntime,
                                           double timeout)
    : sandboxRuntime_(nullptr), hostRuntime_(&hostRuntime), disposed_(false),
      callbackCounter_(0), sandboxFunctionCounter_(0) {
  (void)timeout; // Reserved for future use

  // Create an isolated Hermes runtime for the sandbox
  sandboxRuntime_ = facebook::hermes::makeHermesRuntime();

  if (!sandboxRuntime_) {
    throw jsi::JSError(hostRuntime, "Failed to create Hermes sandbox runtime");
  }

  // Inject console shim into sandbox
  auto consoleObj = jsi::Object(*sandboxRuntime_);

  auto logFn = jsi::Function::createFromHostFunction(
      *sandboxRuntime_,
      jsi::PropNameID::forAscii(*sandboxRuntime_, "log"),
      1,
      [](jsi::Runtime &rt, const jsi::Value &, const jsi::Value *args,
         size_t count) -> jsi::Value {
        NSMutableString *msg = [NSMutableString string];
        for (size_t i = 0; i < count; i++) {
          if (i > 0) [msg appendString:@" "];
          if (args[i].isString()) {
            [msg appendString:[NSString stringWithUTF8String:args[i].getString(rt).utf8(rt).c_str()]];
          } else if (args[i].isNumber()) {
            [msg appendFormat:@"%g", args[i].getNumber()];
          } else if (args[i].isBool()) {
            [msg appendString:args[i].getBool() ? @"true" : @"false"];
          } else if (args[i].isNull()) {
            [msg appendString:@"null"];
          } else if (args[i].isUndefined()) {
            [msg appendString:@"undefined"];
          } else {
            [msg appendString:@"[object]"];
          }
        }
        NSLog(@"[HermesSandbox] %@", msg);
        return jsi::Value::undefined();
      });

  consoleObj.setProperty(*sandboxRuntime_, "log", std::move(logFn));
  consoleObj.setProperty(*sandboxRuntime_, "warn",
      jsi::Function::createFromHostFunction(
          *sandboxRuntime_, jsi::PropNameID::forAscii(*sandboxRuntime_, "warn"), 1,
          [](jsi::Runtime &rt, const jsi::Value &, const jsi::Value *args, size_t count) {
            if (count > 0 && args[0].isString()) {
              NSLog(@"[HermesSandbox][WARN] %s", args[0].getString(rt).utf8(rt).c_str());
            }
            return jsi::Value::undefined();
          }));
  consoleObj.setProperty(*sandboxRuntime_, "error",
      jsi::Function::createFromHostFunction(
          *sandboxRuntime_, jsi::PropNameID::forAscii(*sandboxRuntime_, "error"), 1,
          [](jsi::Runtime &rt, const jsi::Value &, const jsi::Value *args, size_t count) {
            if (count > 0 && args[0].isString()) {
              NSLog(@"[HermesSandbox][ERROR] %s", args[0].getString(rt).utf8(rt).c_str());
            }
            return jsi::Value::undefined();
          }));

  sandboxRuntime_->global().setProperty(*sandboxRuntime_, "console",
                                        std::move(consoleObj));

  NSLog(@"[HermesSandbox] Created new Hermes sandbox context");
}

HermesSandboxContext::~HermesSandboxContext() { dispose(); }

void HermesSandboxContext::dispose() {
  std::lock_guard<std::recursive_mutex> lock(mutex_);
  if (disposed_)
    return;
  disposed_ = true;

  callbacks_.clear();
  sandboxFunctions_.clear();
  sandboxRuntime_.reset();
  NSLog(@"[HermesSandbox] Disposed sandbox context");
}

jsi::Value HermesSandboxContext::get(jsi::Runtime &rt,
                                     const jsi::PropNameID &name) {
  std::string propName = name.utf8(rt);

  if (propName == "eval") {
    return jsi::Function::createFromHostFunction(
        rt, name, 1,
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

  if (propName == "evalBytecode") {
    return jsi::Function::createFromHostFunction(
        rt, name, 1,
        [this](jsi::Runtime &rt, const jsi::Value &thisVal,
               const jsi::Value *args, size_t count) -> jsi::Value {
          (void)thisVal;
          if (count < 1 || !args[0].isObject()) {
            throw jsi::JSError(rt, "evalBytecode requires an ArrayBuffer argument");
          }
          jsi::Object obj = args[0].asObject(rt);
          if (!obj.isArrayBuffer(rt)) {
            throw jsi::JSError(rt, "evalBytecode requires an ArrayBuffer argument");
          }
          jsi::ArrayBuffer ab = obj.getArrayBuffer(rt);
          return this->evalBytecode(rt, ab.data(rt), ab.size(rt));
        });
  }

  if (propName == "setGlobal") {
    return jsi::Function::createFromHostFunction(
        rt, name, 2,
        [this](jsi::Runtime &rt, const jsi::Value &thisVal,
               const jsi::Value *args, size_t count) -> jsi::Value {
          (void)thisVal;
          if (count < 2 || !args[0].isString()) {
            throw jsi::JSError(rt, "setGlobal requires (name: string, value: any)");
          }
          std::string globalName = args[0].asString(rt).utf8(rt);
          this->setGlobal(rt, globalName, args[1]);
          return jsi::Value::undefined();
        });
  }

  if (propName == "getGlobal") {
    return jsi::Function::createFromHostFunction(
        rt, name, 1,
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

  return jsi::Value::undefined();
}

void HermesSandboxContext::set(jsi::Runtime &rt, const jsi::PropNameID &name,
                               const jsi::Value &value) {
  (void)rt;
  (void)name;
  (void)value;
  // Read-only
}

std::vector<jsi::PropNameID>
HermesSandboxContext::getPropertyNames(jsi::Runtime &rt) {
  std::vector<jsi::PropNameID> props;
  props.push_back(jsi::PropNameID::forUtf8(rt, "eval"));
  props.push_back(jsi::PropNameID::forUtf8(rt, "evalBytecode"));
  props.push_back(jsi::PropNameID::forUtf8(rt, "setGlobal"));
  props.push_back(jsi::PropNameID::forUtf8(rt, "getGlobal"));
  props.push_back(jsi::PropNameID::forUtf8(rt, "dispose"));
  return props;
}

jsi::Value HermesSandboxContext::eval(jsi::Runtime &rt,
                                      const std::string &code) {
  std::lock_guard<std::recursive_mutex> lock(mutex_);

  if (disposed_) {
    throw jsi::JSError(rt, "Context has been disposed");
  }

  try {
    jsi::Value result =
        sandboxRuntime_->evaluateJavaScript(
            std::make_shared<jsi::StringBuffer>(code), "<sandbox>");
    return sandboxToHost(*sandboxRuntime_, rt, result);
  } catch (const jsi::JSError &e) {
    throw jsi::JSError(rt, std::string("[HermesSandbox] ") + e.what());
  } catch (const std::exception &e) {
    throw jsi::JSError(rt, std::string("[HermesSandbox] ") + e.what());
  }
}

// Custom buffer adapter for bytecode
class BytecodeBuffer : public jsi::Buffer {
public:
  BytecodeBuffer(const uint8_t *data, size_t size)
      : data_(new uint8_t[size]), size_(size) {
    memcpy(data_, data, size);
  }

  ~BytecodeBuffer() override {
    delete[] data_;
  }

  size_t size() const override {
    return size_;
  }

  const uint8_t *data() const override {
    return data_;
  }

private:
  uint8_t *data_;
  size_t size_;
};

jsi::Value HermesSandboxContext::evalBytecode(jsi::Runtime &rt,
                                               const uint8_t *bytecode,
                                               size_t size) {
  std::lock_guard<std::recursive_mutex> lock(mutex_);

  if (disposed_) {
    throw jsi::JSError(rt, "Context has been disposed");
  }

  if (bytecode == nullptr || size == 0) {
    throw jsi::JSError(rt, "evalBytecode: invalid bytecode (null or empty)");
  }

  // Validate Hermes bytecode magic number (first 4 bytes)
  // Hermes bytecode starts with: 0x1F 0x73 0x63 0x1E (magic) or similar
  // Actually the magic is platform-dependent, so we just try to load it
  // and let Hermes validate

  try {
    // prepareJavaScript can handle both source and bytecode
    // For bytecode, it detects the format and skips parsing
    auto prepared = sandboxRuntime_->prepareJavaScript(
        std::make_unique<BytecodeBuffer>(bytecode, size),
        "<precompiled>");

    jsi::Value result = sandboxRuntime_->evaluatePreparedJavaScript(prepared);
    return sandboxToHost(*sandboxRuntime_, rt, result);
  } catch (const jsi::JSError &e) {
    throw jsi::JSError(rt, std::string("[HermesSandbox] evalBytecode: ") + e.what());
  } catch (const std::exception &e) {
    throw jsi::JSError(rt, std::string("[HermesSandbox] evalBytecode: ") + e.what());
  }
}

void HermesSandboxContext::setGlobal(jsi::Runtime &rt, const std::string &name,
                                     const jsi::Value &value) {
  std::lock_guard<std::recursive_mutex> lock(mutex_);

  if (disposed_) {
    throw jsi::JSError(rt, "Context has been disposed");
  }

  jsi::Value sandboxValue = hostToSandbox(rt, *sandboxRuntime_, value);
  sandboxRuntime_->global().setProperty(*sandboxRuntime_, name.c_str(),
                                        std::move(sandboxValue));
}

jsi::Value HermesSandboxContext::getGlobal(jsi::Runtime &rt,
                                           const std::string &name) {
  std::lock_guard<std::recursive_mutex> lock(mutex_);

  if (disposed_) {
    throw jsi::JSError(rt, "Context has been disposed");
  }

  jsi::Value sandboxValue =
      sandboxRuntime_->global().getProperty(*sandboxRuntime_, name.c_str());
  return sandboxToHost(*sandboxRuntime_, rt, sandboxValue);
}

// MARK: - HermesSandboxRuntime Implementation

HermesSandboxRuntime::HermesSandboxRuntime(jsi::Runtime &hostRuntime,
                                           double timeout)
    : hostRuntime_(&hostRuntime), timeout_(timeout), disposed_(false) {}

HermesSandboxRuntime::~HermesSandboxRuntime() { dispose(); }

void HermesSandboxRuntime::dispose() {
  std::lock_guard<std::recursive_mutex> lock(mutex_);
  if (disposed_)
    return;
  disposed_ = true;

  for (auto &ctx : contexts_) {
    ctx->dispose();
  }
  contexts_.clear();
}

jsi::Value HermesSandboxRuntime::get(jsi::Runtime &rt,
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

void HermesSandboxRuntime::set(jsi::Runtime &rt, const jsi::PropNameID &name,
                               const jsi::Value &value) {
  (void)rt;
  (void)name;
  (void)value;
  // Read-only
}

std::vector<jsi::PropNameID>
HermesSandboxRuntime::getPropertyNames(jsi::Runtime &rt) {
  std::vector<jsi::PropNameID> props;
  props.push_back(jsi::PropNameID::forUtf8(rt, "createContext"));
  props.push_back(jsi::PropNameID::forUtf8(rt, "dispose"));
  return props;
}

jsi::Value HermesSandboxRuntime::createContext(jsi::Runtime &rt) {
  std::lock_guard<std::recursive_mutex> lock(mutex_);

  if (disposed_) {
    throw jsi::JSError(rt, "Runtime has been disposed");
  }

  auto context = std::make_shared<HermesSandboxContext>(*hostRuntime_, timeout_);
  contexts_.push_back(context);

  return jsi::Object::createFromHostObject(rt, context);
}

// MARK: - HermesSandboxModule Implementation

HermesSandboxModule::HermesSandboxModule(jsi::Runtime &runtime) {
  (void)runtime;
}

HermesSandboxModule::~HermesSandboxModule() {}

jsi::Value HermesSandboxModule::get(jsi::Runtime &rt,
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

          auto runtime = std::make_shared<HermesSandboxRuntime>(rt, timeout);
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

void HermesSandboxModule::set(jsi::Runtime &rt, const jsi::PropNameID &name,
                              const jsi::Value &value) {
  (void)rt;
  (void)name;
  (void)value;
  // Read-only
}

std::vector<jsi::PropNameID>
HermesSandboxModule::getPropertyNames(jsi::Runtime &rt) {
  std::vector<jsi::PropNameID> props;
  props.push_back(jsi::PropNameID::forUtf8(rt, "createRuntime"));
  props.push_back(jsi::PropNameID::forUtf8(rt, "isAvailable"));
  return props;
}

void HermesSandboxModule::install(jsi::Runtime &runtime) {
  auto module = std::make_shared<HermesSandboxModule>(runtime);
  jsi::Object moduleObj = jsi::Object::createFromHostObject(runtime, module);
  runtime.global().setProperty(runtime, "__HermesSandboxJSI",
                               std::move(moduleObj));
  NSLog(@"[HermesSandbox] Installed __HermesSandboxJSI");
}

// Wrapper function for external linkage (avoids JSValue symbol conflicts)
void installHermesSandbox(jsi::Runtime &runtime) {
  HermesSandboxModule::install(runtime);
}

} // namespace hermes_sandbox
