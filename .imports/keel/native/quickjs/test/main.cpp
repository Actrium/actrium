/*
 * QuickJS Sandbox Test Runner
 *
 * This minimal C++ runner:
 * 1. Creates a JSI Runtime using QuickJS
 * 2. Installs the sandbox module
 * 3. Runs the JavaScript test suite
 *
 * The actual tests are in sandbox_test.js - testing JS from JS,
 * which reflects the real usage scenario.
 */

#include "../src/QuickJSRuntimeFactory.h"
#include "../src/QuickJSSandboxJSI.h"
#include <fstream>
#include <iostream>
#include <sstream>

using namespace facebook;

std::string readFile(const std::string &path) {
  std::ifstream file(path);
  if (!file.is_open()) {
    throw std::runtime_error("Failed to open file: " + path);
  }
  std::stringstream buffer;
  buffer << file.rdbuf();
  return buffer.str();
}

// Install a minimal console.log for the host runtime
void installConsole(jsi::Runtime &runtime) {
  auto console = jsi::Object(runtime);

  auto log = jsi::Function::createFromHostFunction(
      runtime, jsi::PropNameID::forAscii(runtime, "log"), 1,
      [](jsi::Runtime &rt, const jsi::Value &, const jsi::Value *args,
         size_t count) -> jsi::Value {
        for (size_t i = 0; i < count; i++) {
          if (i > 0)
            std::cout << " ";
          if (args[i].isString()) {
            std::cout << args[i].asString(rt).utf8(rt);
          } else if (args[i].isNumber()) {
            std::cout << args[i].asNumber();
          } else if (args[i].isBool()) {
            std::cout << (args[i].getBool() ? "true" : "false");
          } else if (args[i].isNull()) {
            std::cout << "null";
          } else if (args[i].isUndefined()) {
            std::cout << "undefined";
          } else {
            std::cout << args[i].toString(rt).utf8(rt);
          }
        }
        std::cout << std::endl;
        return jsi::Value::undefined();
      });

  console.setProperty(runtime, "log", std::move(log));

  // Alias warn/error to log
  console.setProperty(runtime, "warn", console.getProperty(runtime, "log"));
  console.setProperty(runtime, "error", console.getProperty(runtime, "log"));

  runtime.global().setProperty(runtime, "console", std::move(console));
}

int main(int argc, const char *argv[]) {
  (void)argc;
  (void)argv;

  std::cout << "==========================================" << std::endl;
  std::cout << "QuickJS Sandbox Test Runner" << std::endl;
  std::cout << "(Using QuickJS Runtime)" << std::endl;
  std::cout << "==========================================" << std::endl;

  try {
    // 1. Create the host JSI runtime using QuickJS factory
    auto runtime = qjs::createQuickJSRuntime("");

    // 2. Install console for output
    installConsole(*runtime);

    // 3. Install the sandbox module onto the runtime
    quickjs_sandbox::QuickJSSandboxModule::install(*runtime);

    // 4. Find and load the JavaScript test file
    std::vector<std::string> searchPaths = {
        "test/sandbox_test.js", "./test/sandbox_test.js",
        "../test/sandbox_test.js", "sandbox_test.js"};

    std::string testCode;
    bool found = false;

    for (const auto &path : searchPaths) {
      try {
        testCode = readFile(path);
        found = true;
        std::cout << "Loaded test file: " << path << std::endl;
        break;
      } catch (...) {
        // Try next path
      }
    }

    if (!found) {
      std::cerr << "Error: Could not find sandbox_test.js" << std::endl;
      return 1;
    }

    // 5. Run the JavaScript tests
    auto buffer = std::make_shared<jsi::StringBuffer>(testCode);
    jsi::Value result = runtime->evaluateJavaScript(buffer, "sandbox_test.js");

    // 6. Check result (the JS test returns true/false)
    if (result.isBool() && result.getBool()) {
      return 0; // Success
    } else {
      return 1; // Test failures
    }

  } catch (const jsi::JSError &e) {
    std::cerr << "JavaScript Error: " << e.getMessage() << std::endl;
    std::cerr << "Stack: " << e.getStack() << std::endl;
    return 1;
  } catch (const std::exception &e) {
    std::cerr << "Error: " << e.what() << std::endl;
    return 1;
  }

  return 0;
}
