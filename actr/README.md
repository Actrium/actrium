# Actr

Actr is Actrium's foundational actor runtime, WebRTC transport, and protocol
layer. This directory owns the component boundary and integration metadata.

The public Rust facade lives at `../crates/actr/`. Other buildable Actr units
live under `../crates/actr-*` and `../packages/actr-*`; component-specific
developer tools live under `../tools/actr/`. Actr must remain usable without a
platform application, Keel, or Loran.
