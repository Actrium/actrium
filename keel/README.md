# Keel

Keel is Actrium's sandbox UI framework. This directory owns its component
documentation, contribution policy, security policy, and release notes.

The current TypeScript and native implementation remains one coherent package
under [`../packages/keel`](../packages/keel). Its reusable Rust guest SDK lives
under [`../crates/keel-native-guest`](../crates/keel-native-guest), shared wire
contracts live under [`../contracts/sandbox/sandbox`](../contracts/sandbox/sandbox), and demo
applications and guest workloads live under [`../examples/keel`](../examples/keel).

Keel may depend on Actr contracts but not on product-specific Actrium state.
