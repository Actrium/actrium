# JSC Sandbox Makefile
# A standalone JavaScriptCore sandbox using JSI

# Compiler settings
CXX = clang++
CC = clang

# Directories
SRC_DIR = src
JSI_DIR = ../jsi
TEST_DIR = test
BUILD_DIR = build

# Source files
JSI_SOURCES = $(JSI_DIR)/jsi.cpp
SRC_SOURCES = $(SRC_DIR)/JSCSandboxJSI.mm $(SRC_DIR)/JSCRuntime.mm
TEST_SOURCES = $(TEST_DIR)/main.mm

# Object files
JSI_OBJECTS = $(BUILD_DIR)/jsi.o
SRC_OBJECTS = $(BUILD_DIR)/JSCSandboxJSI.o $(BUILD_DIR)/JSCRuntime.o
TEST_OBJECTS = $(BUILD_DIR)/main.o

ALL_OBJECTS = $(JSI_OBJECTS) $(SRC_OBJECTS) $(TEST_OBJECTS)

# Output
TEST_BINARY = $(BUILD_DIR)/jsc_sandbox_test

# Extra flags (can be overridden for ASan, etc.)
EXTRA_CXXFLAGS ?=
EXTRA_LDFLAGS ?=

# Compiler flags
CXXFLAGS = -std=c++17 -O2 -Wall -Wextra
CXXFLAGS += -I.
CXXFLAGS += -I..
CXXFLAGS += -fvisibility=hidden
CXXFLAGS += -fno-rtti
CXXFLAGS += $(EXTRA_CXXFLAGS)

# Objective-C++ flags
OBJCXXFLAGS = $(CXXFLAGS) -fobjc-arc

# JSCRuntime needs RTTI (Meta's implementation uses dynamic_cast)
JSCRUNTIME_FLAGS = -std=c++17 -O2 -Wall -Wextra -I. -I.. -fvisibility=hidden -fobjc-arc $(EXTRA_CXXFLAGS)

# Linker flags for macOS
LDFLAGS = -framework JavaScriptCore
LDFLAGS += -framework Foundation
LDFLAGS += $(EXTRA_LDFLAGS)

# Default target
all: $(TEST_BINARY)

# Create build directory
$(BUILD_DIR):
	mkdir -p $(BUILD_DIR)

# Compile JSI source (needs RTTI for Meta's JSCRuntime)
$(BUILD_DIR)/jsi.o: $(JSI_DIR)/jsi.cpp $(JSI_DIR)/jsi.h $(JSI_DIR)/jsi-inl.h $(JSI_DIR)/instrumentation.h | $(BUILD_DIR)
	$(CXX) -std=c++17 -O2 -Wall -Wextra -I. -I.. -fvisibility=hidden -c $< -o $@

# Compile sandbox source
$(BUILD_DIR)/JSCSandboxJSI.o: $(SRC_DIR)/JSCSandboxJSI.mm $(SRC_DIR)/JSCSandboxJSI.h | $(BUILD_DIR)
	$(CXX) $(OBJCXXFLAGS) -c $< -o $@

# Compile JSCRuntime (needs RTTI for Meta's dynamic_cast)
$(BUILD_DIR)/JSCRuntime.o: $(SRC_DIR)/JSCRuntime.mm $(SRC_DIR)/JSCRuntime.h | $(BUILD_DIR)
	$(CXX) $(JSCRUNTIME_FLAGS) -c $< -o $@

# Compile test main
$(BUILD_DIR)/main.o: $(TEST_DIR)/main.mm | $(BUILD_DIR)
	$(CXX) $(OBJCXXFLAGS) -c $< -o $@

# Link test binary
$(TEST_BINARY): $(ALL_OBJECTS)
	$(CXX) $(ALL_OBJECTS) $(LDFLAGS) -o $@

# Run tests
test: $(TEST_BINARY)
	@echo "Running JSC Sandbox tests..."
	@./$(TEST_BINARY)

# Clean build artifacts
clean:
	rm -rf $(BUILD_DIR)

# Debug build
debug: CXXFLAGS += -g -O0 -DDEBUG
debug: OBJCXXFLAGS += -g -O0 -DDEBUG
debug: clean all

# Help
help:
	@echo "JSC Sandbox Build System"
	@echo ""
	@echo "Targets:"
	@echo "  all      - Build the test binary (default)"
	@echo "  test     - Build and run tests"
	@echo "  clean    - Remove build artifacts"
	@echo "  debug    - Build with debug symbols"
	@echo "  help     - Show this message"
	@echo ""
	@echo "Usage:"
	@echo "  make         - Build the project"
	@echo "  make test    - Build and run tests"
	@echo "  make clean   - Clean build directory"

.PHONY: all test clean debug help
