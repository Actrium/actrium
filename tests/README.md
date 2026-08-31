# Cross-layer tests

Tests that span subsystem boundaries live here. Tests owned by one crate,
package, or application remain beside that implementation.

- **conformance/** verifies protocol and SDK behavior across implementations.
- **integration/** verifies two or more layers together.
- **e2e/** verifies complete Actrium journeys on supported platforms.
- **fixtures/** contains shared deterministic test inputs.
