# Keel

Keel is Actrium's sandbox UI framework. This directory owns its component
boundary and integration metadata.

The trusted host, untrusted guest API, execution backends, SDK, and developer
tools are reusable packages under `../packages/keel-*`; Rust guest support, if
needed, lives under `../crates/keel-*`. Keel may depend on Actr contracts but
not on product-specific Actrium state.
