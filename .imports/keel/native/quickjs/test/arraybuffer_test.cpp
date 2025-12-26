/*
 * ArrayBuffer JSI Implementation Test
 * Tests createArrayBuffer, size(), and data() methods
 */

#include "../src/QuickJSRuntime.h"
#include "../src/QuickJSRuntimeFactory.h"
#include <iostream>
#include <cstring>

using namespace facebook;

// Simple MutableBuffer implementation for testing
class TestBuffer : public jsi::MutableBuffer {
private:
  std::vector<uint8_t> data_;

public:
  TestBuffer(size_t size) : data_(size, 0) {}

  TestBuffer(std::initializer_list<uint8_t> init) : data_(init) {}

  size_t size() const override { return data_.size(); }

  uint8_t *data() override { return data_.data(); }
};

// Test 1: Create ArrayBuffer and check size
void testCreateArrayBuffer() {
  std::cout << "\n=== Test 1: Create ArrayBuffer ===" << std::endl;
  auto runtime = qjs::createQuickJSRuntime("");

  auto buffer = std::make_shared<TestBuffer>(std::initializer_list<uint8_t>{
      1, 2, 3, 4, 5});
  auto arrayBuffer = runtime->createArrayBuffer(buffer);

  size_t bufferSize = arrayBuffer.size(*runtime);
  std::cout << "Created ArrayBuffer size: " << bufferSize << std::endl;

  if (bufferSize == 5) {
    std::cout << "✓ Size check passed" << std::endl;
  } else {
    std::cout << "✗ Size check failed: expected 5, got " << bufferSize
              << std::endl;
  }
}

// Test 2: Access ArrayBuffer data
void testArrayBufferData() {
  std::cout << "\n=== Test 2: ArrayBuffer Data Access ===" << std::endl;
  auto runtime = qjs::createQuickJSRuntime("");

  auto buffer = std::make_shared<TestBuffer>(
      std::initializer_list<uint8_t>{10, 20, 30, 40, 50});
  auto arrayBuffer = runtime->createArrayBuffer(buffer);

  uint8_t *data = arrayBuffer.data(*runtime);
  std::cout << "ArrayBuffer data: [";
  for (size_t i = 0; i < 5; i++) {
    std::cout << (int)data[i];
    if (i < 4)
      std::cout << ", ";
  }
  std::cout << "]" << std::endl;

  bool passed = true;
  uint8_t expected[] = {10, 20, 30, 40, 50};
  for (size_t i = 0; i < 5; i++) {
    if (data[i] != expected[i]) {
      std::cout << "✗ Data check failed at index " << i << ": expected "
                << (int)expected[i] << ", got " << (int)data[i] << std::endl;
      passed = false;
    }
  }

  if (passed) {
    std::cout << "✓ Data check passed" << std::endl;
  }
}

// Test 3: ArrayBuffer from JavaScript
void testArrayBufferFromJS() {
  std::cout << "\n=== Test 3: ArrayBuffer from JavaScript ===" << std::endl;
  auto runtime = qjs::createQuickJSRuntime("");

  // Create ArrayBuffer in JS
  auto code = std::make_shared<jsi::StringBuffer>(
      "var buffer = new ArrayBuffer(8);"
      "var view = new Uint8Array(buffer);"
      "view[0] = 100;"
      "view[1] = 101;"
      "view[2] = 102;"
      "view[7] = 200;"
      "buffer");

  auto result = runtime->evaluateJavaScript(code, "test.js");

  if (result.isObject()) {
    auto obj = result.getObject(*runtime);
    if (obj.isArrayBuffer(*runtime)) {
      auto arrayBuffer = obj.getArrayBuffer(*runtime);

      size_t size = arrayBuffer.size(*runtime);
      uint8_t *data = arrayBuffer.data(*runtime);

      std::cout << "ArrayBuffer size: " << size << std::endl;
      std::cout << "ArrayBuffer data: [" << (int)data[0] << ", "
                << (int)data[1] << ", " << (int)data[2] << ", ..., "
                << (int)data[7] << "]" << std::endl;

      if (size == 8 && data[0] == 100 && data[1] == 101 && data[2] == 102 &&
          data[7] == 200) {
        std::cout << "✓ JS ArrayBuffer test passed" << std::endl;
      } else {
        std::cout << "✗ JS ArrayBuffer test failed" << std::endl;
      }
    } else {
      std::cout << "✗ Result is not an ArrayBuffer" << std::endl;
    }
  } else {
    std::cout << "✗ Result is not an object" << std::endl;
  }
}

// Test 4: Pass ArrayBuffer to JavaScript
void testArrayBufferToJS() {
  std::cout << "\n=== Test 4: Pass ArrayBuffer to JavaScript ===" << std::endl;
  auto runtime = qjs::createQuickJSRuntime("");

  // Create ArrayBuffer in C++
  auto buffer = std::make_shared<TestBuffer>(
      std::initializer_list<uint8_t>{5, 10, 15, 20});
  auto arrayBuffer = runtime->createArrayBuffer(buffer);

  // Set as global
  runtime->global().setProperty(*runtime, "myBuffer", std::move(arrayBuffer));

  // Access from JS
  auto code = std::make_shared<jsi::StringBuffer>(
      "var view = new Uint8Array(myBuffer);"
      "var sum = 0;"
      "for (var i = 0; i < view.length; i++) {"
      "  sum += view[i];"
      "}"
      "sum");

  auto result = runtime->evaluateJavaScript(code, "test.js");

  if (result.isNumber()) {
    double sum = result.getNumber();
    std::cout << "Sum from JS: " << sum << std::endl;

    // Expected: 5 + 10 + 15 + 20 = 50
    if (sum == 50) {
      std::cout << "✓ ArrayBuffer to JS test passed" << std::endl;
    } else {
      std::cout << "✗ ArrayBuffer to JS test failed: expected 50, got " << sum
                << std::endl;
    }
  } else {
    std::cout << "✗ Result is not a number" << std::endl;
  }
}

// Test 5: isArrayBuffer check
void testIsArrayBuffer() {
  std::cout << "\n=== Test 5: isArrayBuffer Check ===" << std::endl;
  auto runtime = qjs::createQuickJSRuntime("");

  // Create ArrayBuffer
  auto buffer = std::make_shared<TestBuffer>(10);
  auto arrayBuffer = runtime->createArrayBuffer(buffer);
  runtime->global().setProperty(*runtime, "ab", std::move(arrayBuffer));

  // Create regular object
  jsi::Object obj(*runtime);
  runtime->global().setProperty(*runtime, "obj", std::move(obj));

  // Check isArrayBuffer
  auto checkCode = std::make_shared<jsi::StringBuffer>(
      "var abIsAB = ab instanceof ArrayBuffer;"
      "var objIsAB = obj instanceof ArrayBuffer;"
      "abIsAB && !objIsAB");

  auto result = runtime->evaluateJavaScript(checkCode, "test.js");

  if (result.isBool() && result.getBool()) {
    std::cout << "✓ isArrayBuffer check passed" << std::endl;
  } else {
    std::cout << "✗ isArrayBuffer check failed" << std::endl;
  }

  // Also test C++ API
  auto abObj = runtime->global().getPropertyAsObject(*runtime, "ab");
  auto regularObj = runtime->global().getPropertyAsObject(*runtime, "obj");

  if (abObj.isArrayBuffer(*runtime) && !regularObj.isArrayBuffer(*runtime)) {
    std::cout << "✓ C++ isArrayBuffer API check passed" << std::endl;
  } else {
    std::cout << "✗ C++ isArrayBuffer API check failed" << std::endl;
  }
}

// Test 6: Multiple ArrayBuffers
void testMultipleArrayBuffers() {
  std::cout << "\n=== Test 6: Multiple ArrayBuffers ===" << std::endl;
  auto runtime = qjs::createQuickJSRuntime("");

  auto buffer1 =
      std::make_shared<TestBuffer>(std::initializer_list<uint8_t>{1, 2, 3});
  auto buffer2 = std::make_shared<TestBuffer>(
      std::initializer_list<uint8_t>{10, 20, 30, 40});
  auto buffer3 = std::make_shared<TestBuffer>(
      std::initializer_list<uint8_t>{100, 101});

  auto ab1 = runtime->createArrayBuffer(buffer1);
  auto ab2 = runtime->createArrayBuffer(buffer2);
  auto ab3 = runtime->createArrayBuffer(buffer3);

  size_t size1 = ab1.size(*runtime);
  size_t size2 = ab2.size(*runtime);
  size_t size3 = ab3.size(*runtime);

  std::cout << "AB1 size: " << size1 << std::endl;
  std::cout << "AB2 size: " << size2 << std::endl;
  std::cout << "AB3 size: " << size3 << std::endl;

  if (size1 == 3 && size2 == 4 && size3 == 2) {
    std::cout << "✓ Multiple ArrayBuffers test passed" << std::endl;
  } else {
    std::cout << "✗ Multiple ArrayBuffers test failed" << std::endl;
  }
}

// Test 7: Empty ArrayBuffer
void testEmptyArrayBuffer() {
  std::cout << "\n=== Test 7: Empty ArrayBuffer ===" << std::endl;
  auto runtime = qjs::createQuickJSRuntime("");

  auto buffer = std::make_shared<TestBuffer>(0);
  auto arrayBuffer = runtime->createArrayBuffer(buffer);

  size_t size = arrayBuffer.size(*runtime);

  std::cout << "Empty ArrayBuffer size: " << size << std::endl;

  if (size == 0) {
    std::cout << "✓ Empty ArrayBuffer test passed" << std::endl;
  } else {
    std::cout << "✗ Empty ArrayBuffer test failed" << std::endl;
  }
}

// Test 8: Large ArrayBuffer
void testLargeArrayBuffer() {
  std::cout << "\n=== Test 8: Large ArrayBuffer ===" << std::endl;
  auto runtime = qjs::createQuickJSRuntime("");

  const size_t largeSize = 1024 * 1024; // 1MB
  auto buffer = std::make_shared<TestBuffer>(largeSize);

  // Fill with pattern
  uint8_t *data = buffer->data();
  for (size_t i = 0; i < largeSize; i++) {
    data[i] = static_cast<uint8_t>(i % 256);
  }

  auto arrayBuffer = runtime->createArrayBuffer(buffer);

  size_t size = arrayBuffer.size(*runtime);
  uint8_t *abData = arrayBuffer.data(*runtime);

  std::cout << "Large ArrayBuffer size: " << size << std::endl;

  // Verify pattern
  bool passed = true;
  for (size_t i = 0; i < largeSize && passed; i++) {
    if (abData[i] != static_cast<uint8_t>(i % 256)) {
      std::cout << "✗ Data mismatch at index " << i << std::endl;
      passed = false;
    }
  }

  if (passed && size == largeSize) {
    std::cout << "✓ Large ArrayBuffer test passed" << std::endl;
  } else {
    std::cout << "✗ Large ArrayBuffer test failed" << std::endl;
  }
}

int main(int argc, const char *argv[]) {
  (void)argc;
  (void)argv;

  std::cout << "==========================================" << std::endl;
  std::cout << "QuickJS ArrayBuffer JSI Test" << std::endl;
  std::cout << "==========================================" << std::endl;

  try {
    testCreateArrayBuffer();
    testArrayBufferData();
    testArrayBufferFromJS();
    testArrayBufferToJS();
    testIsArrayBuffer();
    testMultipleArrayBuffers();
    testEmptyArrayBuffer();
    testLargeArrayBuffer();

    std::cout << "\n==========================================" << std::endl;
    std::cout << "All ArrayBuffer tests completed!" << std::endl;
    std::cout << "==========================================" << std::endl;

  } catch (const jsi::JSError &e) {
    std::cerr << "JS Error: " << e.getMessage() << std::endl;
    return 1;
  } catch (const std::exception &e) {
    std::cerr << "Error: " << e.what() << std::endl;
    return 1;
  }

  return 0;
}
