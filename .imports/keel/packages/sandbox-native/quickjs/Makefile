# QuickJS Sandbox Makefile
# A standalone QuickJS sandbox using JSI

# Compiler settings
CXX = clang++
CC = clang

# Directories
VENDOR_DIR = vendor
SRC_DIR = src
JSI_DIR = ../jsi
TEST_DIR = test
BUILD_DIR = build

# QuickJS engine source files (C) - third-party
VENDOR_C_SOURCES = \
	$(VENDOR_DIR)/quickjs.c \
	$(VENDOR_DIR)/libregexp.c \
	$(VENDOR_DIR)/libunicode.c \
	$(VENDOR_DIR)/cutils.c \
	$(VENDOR_DIR)/libbf.c

# JSI binding source files (C++) - our code
SRC_CXX_SOURCES = \
	$(SRC_DIR)/QuickJSRuntime.cpp \
	$(SRC_DIR)/QuickJSRuntimeFactory.cpp \
	$(SRC_DIR)/QuickJSPointerValue.cpp \
	$(SRC_DIR)/JSIValueConverter.cpp \
	$(SRC_DIR)/HostProxy.cpp \
	$(SRC_DIR)/QuickJSInstrumentation.cpp \
	$(SRC_DIR)/QuickJSSandboxJSI.cpp

# JSI source files
JSI_SOURCES = $(JSI_DIR)/jsi.cpp

# Test source files
TEST_SOURCES = $(TEST_DIR)/main.cpp

# Object files
VENDOR_C_OBJECTS = $(patsubst $(VENDOR_DIR)/%.c,$(BUILD_DIR)/%.o,$(VENDOR_C_SOURCES))
SRC_CXX_OBJECTS = $(patsubst $(SRC_DIR)/%.cpp,$(BUILD_DIR)/%.o,$(SRC_CXX_SOURCES))
JSI_OBJECTS = $(BUILD_DIR)/jsi.o
TEST_OBJECTS = $(BUILD_DIR)/main.o

ALL_OBJECTS = $(VENDOR_C_OBJECTS) $(SRC_CXX_OBJECTS) $(JSI_OBJECTS) $(TEST_OBJECTS)

# Output
TEST_BINARY = $(BUILD_DIR)/quickjs_sandbox_test

# Extra flags (can be overridden for ASan, etc.)
EXTRA_CFLAGS ?=
EXTRA_CXXFLAGS ?=
EXTRA_LDFLAGS ?=

# C compiler flags for QuickJS engine (vendor code - suppress warnings)
CFLAGS = -O2 -w
CFLAGS += -I$(VENDOR_DIR)
CFLAGS += -D_GNU_SOURCE
CFLAGS += -DCONFIG_VERSION=\"2024-01-13\"
CFLAGS += -DCONFIG_BIGNUM
CFLAGS += $(EXTRA_CFLAGS)

# C++ compiler flags
CXXFLAGS = -std=c++17 -O2 -Wall -Wextra
CXXFLAGS += -I.
CXXFLAGS += -I..
CXXFLAGS += -isystem $(VENDOR_DIR)
CXXFLAGS += -isystem ../$(VENDOR_DIR)
CXXFLAGS += -fvisibility=hidden
CXXFLAGS += $(EXTRA_CXXFLAGS)

# Linker flags
LDFLAGS = -lm -lpthread $(EXTRA_LDFLAGS)

# Default target
all: $(TEST_BINARY)

# Create build directory
$(BUILD_DIR):
	mkdir -p $(BUILD_DIR)

# Compile QuickJS C sources (vendor)
$(BUILD_DIR)/quickjs.o: $(VENDOR_DIR)/quickjs.c | $(BUILD_DIR)
	$(CC) $(CFLAGS) -c $< -o $@

$(BUILD_DIR)/libregexp.o: $(VENDOR_DIR)/libregexp.c | $(BUILD_DIR)
	$(CC) $(CFLAGS) -c $< -o $@

$(BUILD_DIR)/libunicode.o: $(VENDOR_DIR)/libunicode.c | $(BUILD_DIR)
	$(CC) $(CFLAGS) -c $< -o $@

$(BUILD_DIR)/cutils.o: $(VENDOR_DIR)/cutils.c | $(BUILD_DIR)
	$(CC) $(CFLAGS) -c $< -o $@

$(BUILD_DIR)/libbf.o: $(VENDOR_DIR)/libbf.c | $(BUILD_DIR)
	$(CC) $(CFLAGS) -c $< -o $@

# Compile JSI binding C++ sources (src)
$(BUILD_DIR)/QuickJSRuntime.o: $(SRC_DIR)/QuickJSRuntime.cpp | $(BUILD_DIR)
	$(CXX) $(CXXFLAGS) -c $< -o $@

$(BUILD_DIR)/QuickJSRuntimeFactory.o: $(SRC_DIR)/QuickJSRuntimeFactory.cpp | $(BUILD_DIR)
	$(CXX) $(CXXFLAGS) -c $< -o $@

$(BUILD_DIR)/QuickJSPointerValue.o: $(SRC_DIR)/QuickJSPointerValue.cpp | $(BUILD_DIR)
	$(CXX) $(CXXFLAGS) -c $< -o $@

$(BUILD_DIR)/JSIValueConverter.o: $(SRC_DIR)/JSIValueConverter.cpp | $(BUILD_DIR)
	$(CXX) $(CXXFLAGS) -c $< -o $@

$(BUILD_DIR)/HostProxy.o: $(SRC_DIR)/HostProxy.cpp | $(BUILD_DIR)
	$(CXX) $(CXXFLAGS) -c $< -o $@

$(BUILD_DIR)/QuickJSInstrumentation.o: $(SRC_DIR)/QuickJSInstrumentation.cpp | $(BUILD_DIR)
	$(CXX) $(CXXFLAGS) -c $< -o $@

$(BUILD_DIR)/QuickJSSandboxJSI.o: $(SRC_DIR)/QuickJSSandboxJSI.cpp $(SRC_DIR)/QuickJSSandboxJSI.h | $(BUILD_DIR)
	$(CXX) $(CXXFLAGS) -c $< -o $@

# Compile JSI source
$(BUILD_DIR)/jsi.o: $(JSI_DIR)/jsi.cpp | $(BUILD_DIR)
	$(CXX) $(CXXFLAGS) -c $< -o $@

# Compile test main
$(BUILD_DIR)/main.o: $(TEST_DIR)/main.cpp | $(BUILD_DIR)
	$(CXX) $(CXXFLAGS) -c $< -o $@

# Compile leak test
$(BUILD_DIR)/leak_test.o: $(TEST_DIR)/leak_test.cpp | $(BUILD_DIR)
	$(CXX) $(CXXFLAGS) -c $< -o $@

# Link test binary
$(TEST_BINARY): $(ALL_OBJECTS)
	$(CXX) $(ALL_OBJECTS) $(LDFLAGS) -o $@

# Leak test objects (exclude main.o, use leak_test.o)
LEAK_TEST_OBJECTS = $(VENDOR_C_OBJECTS) $(SRC_CXX_OBJECTS) $(JSI_OBJECTS) $(BUILD_DIR)/leak_test.o

# Link leak test binary
$(BUILD_DIR)/leak_test: $(LEAK_TEST_OBJECTS)
	$(CXX) $(LEAK_TEST_OBJECTS) $(LDFLAGS) -o $@

leak_test: $(BUILD_DIR)/leak_test
	@echo "Leak test binary built at $(BUILD_DIR)/leak_test"

# Run tests
test: $(TEST_BINARY)
	@echo "Running QuickJS Sandbox tests..."
	@./$(TEST_BINARY)

# Clean build artifacts
clean:
	rm -rf $(BUILD_DIR)

# Debug build
debug: CFLAGS += -g -O0 -DDEBUG
debug: CXXFLAGS += -g -O0 -DDEBUG
debug: clean all

# Help
help:
	@echo "QuickJS Sandbox Build System"
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

.PHONY: all test clean debug help leak_test
