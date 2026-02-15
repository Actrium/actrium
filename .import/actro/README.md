# RustDesk ACTR

[![License](https://img.shields.io/badge/license-GPL--3.0-blue.svg)](https://github.com/kookyleo/rustdesk-actr/blob/main/LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org/)
[![macOS](https://img.shields.io/badge/platform-macOS%2012.0%2B-lightgrey.svg)](https://www.apple.com/macos/)

A modern remote desktop solution built with [ACTR](https://github.com/actor-rtc/actr) WebRTC framework, focused on macOS native experience with virtual display technology.

## 🌟 Features

- **ACTR Transport Layer** - WebRTC-based communication using ACTR framework
- **Virtual Display** - Uses macOS 12.0+ `CGVirtualDisplay` API to prevent recursive screen capture
- **Clean Architecture** - Client/Host separation with shared libraries in Cargo workspace
- **macOS Native** - Native Cocoa UI, optimized for Apple Silicon and Intel Macs
- **Modern Rust** - Built with async/await, Tokio runtime, and modern Rust practices

## 📁 Project Structure

```
rustdesk-actr/
├── Cargo.toml          # Workspace configuration
├── client/             # RustDesk ACTR Client (macOS)
│   ├── src/
│   ├── examples/
│   └── Cargo.toml
├── host/               # RustDesk ACTR Host (headless server)
│   ├── src/
│   ├── examples/
│   └── Cargo.toml
├── libs/               # Shared libraries
│   ├── hbb_common/     # Core utilities, video codec, protobuf
│   ├── scrap/          # Screen capture
│   ├── enigo/          # Input control (keyboard/mouse)
│   └── clipboard/      # Clipboard handling
├── run_test.sh         # All-in-one test script
└── docs/               # Documentation
```

## 🚀 Quick Start

### Prerequisites

#### 1. Rust Toolchain

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup default stable
```

#### 2. System Dependencies

**For macOS with Homebrew:**

```bash
# Install vcpkg (recommended)
brew install vcpkg

# Install required packages via vcpkg
vcpkg install libyuv libvpx aom

# Set environment variable
export VCPKG_ROOT=/opt/homebrew/opt/vcpkg
```

**Alternative for Apple Silicon (M1/M2):**

```bash
# Direct homebrew installation (fallback)
brew install libyuv
# Note: libvpx and aom need manual compilation or vcpkg
```

#### 3. macOS Version

- **Minimum**: macOS 12.0 (Monterey) - required for `CGVirtualDisplay` API
- **Recommended**: macOS 13.0+ (Ventura) for best performance

### Build

```bash
# Clone the repository
git clone https://github.com/kookyleo/rustdesk-actr.git
cd rustdesk-actr

# Build client
cargo build -p rustdesk-client --features actr-transport

# Build host
cargo build -p rustdesk-host --features actr-transport

# Build both in release mode
cargo build --release --features actr-transport
```

### Run Tests

#### All-in-One Test Script (Recommended)

```bash
# Full test (build + run)
./run_test.sh

# Quick test (skip build)
./run_test.sh quick

# View logs
./run_test.sh logs

# Cleanup only
./run_test.sh clean
```

#### Manual Testing

**Terminal 1 - Start Host:**
```bash
cd host
cargo run --example test_virtual_display_protocol --features actr-transport
```

**Terminal 2 - Start Client:**
```bash
cd client
cargo run --example test_virtual_display_client --features actr-transport
```

**Terminal 3 - Monitor Logs:**
```bash
tail -f /tmp/rustdesk_host.log /tmp/rustdesk_client.log
```

### Verification

After running the test:

1. **Virtual Display** - Open System Preferences > Displays, verify a new 1920x1080 display appears
2. **Remote Desktop Window** - A window titled "RustDesk Remote Desktop" should appear
3. **No Recursive Capture** - The window content should NOT show itself (no "hall of mirrors")
4. **Test Pass** - Client log shows "✅ ALL TESTS PASSED"

## 📖 Documentation

- [TESTING.md](client/TESTING.md) - Comprehensive testing guide
- [ARCHITECTURE.md](docs/ARCHITECTURE.md) - System architecture and design
- [API.md](docs/API.md) - ACTR protocol and API documentation

## 🏗️ Architecture

### Transport Layer: ACTR

All network communication uses the ACTR (Actor Runtime) framework:

- **WebRTC-based** - Modern peer-to-peer communication
- **Actor Model** - Structured message passing with route keys
- **Type-safe** - Protobuf message definitions

Route key format: `rustdesk.ServiceName/MethodName`

Example:
```rust
"rustdesk.ConnectionService/Connect"
"rustdesk.VirtualDisplayService/CreateVirtualDisplay"
```

### Client Architecture

```
┌─────────────────────────────────────┐
│   macOS Native UI (Cocoa/Objective-C) │
├─────────────────────────────────────┤
│   Client Workload (ACTR Actor)       │
│   - Video decoding                   │
│   - Input handling                   │
│   - Window management                │
├─────────────────────────────────────┤
│   ACTR Transport Layer               │
│   - WebRTC connections               │
│   - Message routing                  │
└─────────────────────────────────────┘
```

### Host Architecture

```
┌─────────────────────────────────────┐
│   Virtual Display Creation           │
│   (CGVirtualDisplay API)             │
├─────────────────────────────────────┤
│   Host Workload (ACTR Actor)         │
│   - Screen capture                   │
│   - Video encoding                   │
│   - Input simulation                 │
├─────────────────────────────────────┤
│   ACTR Transport Layer               │
│   - WebRTC connections               │
│   - Message routing                  │
└─────────────────────────────────────┘
```

## 🔧 Development

### Workspace Commands

```bash
# Build everything
cargo build --workspace

# Build specific package
cargo build -p rustdesk-client
cargo build -p rustdesk-host

# Run tests
cargo test --workspace

# Check code
cargo clippy --workspace -- -D warnings

# Format code
cargo fmt --all
```

### Feature Flags

- `actr-transport` - Enable ACTR WebRTC transport (required)
- `hwcodec` - Hardware video encoding/decoding
- `screencapturekit` - Use macOS ScreenCaptureKit API
- `use_dasp` - Enable dasp audio processing (default)

### Adding Dependencies

Add shared dependencies to `Cargo.toml` in the workspace root:

```toml
[workspace.dependencies]
your-crate = "1.0"
```

Then use in client or host:

```toml
[dependencies]
your-crate = { workspace = true }
```

## 🐛 Troubleshooting

### Build Errors

**Problem**: `Could not find package in /opt/homebrew/Cellar/libyuv`

**Solution**: Install dependencies via vcpkg or homebrew (see Prerequisites)

**Problem**: `workspace dependencies cannot be optional`

**Solution**: This is already fixed. Optional dependencies (like `dasp`) are defined locally in client/host Cargo.toml, not in workspace dependencies.

### Runtime Errors

**Problem**: `Unknown route: rustdesk.control.*`

**Solution**: Ensure you're using the correct route key format: `rustdesk.ServiceName/MethodName`

**Problem**: Virtual display not created

**Solution**:
1. Check macOS version >= 12.0
2. Grant necessary permissions to the application
3. Check logs in `/tmp/rustdesk_client.log`

### Test Failures

**Problem**: Host fails to start (port already in use)

**Solution**:
```bash
# Kill existing process
lsof -i :21118
kill -9 <PID>

# Or use cleanup script
./run_test.sh clean
```

## 📊 Current Status

### ✅ Core Features (Fully Implemented)

- [x] **ACTR Integration** - WebRTC transport for client/host communication
- [x] **Video Streaming** - VP9/VP8 encoding with fragmentation and reassembly
- [x] **Audio Streaming** - Opus codec with cpal integration
- [x] **Input Control** - Full mouse and keyboard via enigo
- [x] **Clipboard Sync** - Text, images, HTML, and file paths
- [x] **File Transfer** - Chunked async transfer with progress tracking
- [x] **Virtual Display** - macOS CGVirtualDisplay API (no recursive capture)
- [x] **Privacy Mode** - Screen blackout + local input blocking (macOS)
- [x] **Native UI** - macOS Cocoa window with BGRA rendering

### 📅 Future Plans

- [ ] Multi-display support
- [ ] Cross-platform support (Linux, Windows)

## 🤝 Contributing

Contributions are welcome! Please read our [Contributing Guide](CONTRIBUTING.md) before submitting PRs.

### Development Workflow

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes
4. Run tests (`cargo test --workspace`)
5. Format code (`cargo fmt --all`)
6. Run clippy (`cargo clippy --workspace`)
7. Commit your changes (`git commit -m 'Add amazing feature'`)
8. Push to the branch (`git push origin feature/amazing-feature`)
9. Open a Pull Request

## 📜 License

This project is licensed under the GNU General Public License v3.0 - see the [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- [RustDesk](https://github.com/rustdesk/rustdesk) - Original RustDesk project
- [ACTR](https://github.com/actor-rtc/actr) - Modern WebRTC framework
- [rust-scrap](https://github.com/quadrupleslap/scrap) - Screen capture library

## 📧 Contact

- GitHub Issues: [https://github.com/kookyleo/rustdesk-actr/issues](https://github.com/kookyleo/rustdesk-actr/issues)
- Repository: [https://github.com/kookyleo/rustdesk-actr](https://github.com/kookyleo/rustdesk-actr)

---

**Made with ❤️ for the open-source community**
