# Actrix control contracts

This crate generates protobuf types for the Actrix management/control plane. Realm v1 uses
`SIGNER` and `PUBLISHER_REGISTRY` as external terminology; enum values are part of the protocol,
renaming does not change existing tags.

The runtime gRPC contract for Signer is in `signer.v1`. The Authority/Actor network protocol is
in the workspace's `core/protocol`; a second identity model should not be duplicated in this crate.
