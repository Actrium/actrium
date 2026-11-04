## System requirements

Linux `x86_64` / `arm64` binaries are built on GitHub's `ubuntu-latest` runner and require **glibc ≥ 2.39**. On older distributions (e.g. `ubuntu-22.04`, glibc 2.35), build loran from source against the local toolchain: `cargo build --release -p loran-server --features opentelemetry`.

---
