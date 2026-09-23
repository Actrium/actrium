# actr-protocol

Unified protocol, types, and URI parsing for Actor-RTC framework.

## Overview

`actr-protocol` is the protocol foundation layer of the Actor-RTC framework. It defines the standardized underlying communication contracts required for the framework's operation and provides stateless utility functions closely related to these protocols.

## Features

- **Protocol Definitions**: Provides `.proto` files and generated Rust types covering identity, signaling, service discovery, and other framework-level concepts.
- **Core Utilities**: Provides necessary extensions for core types, such as string parsing/formatting for `ActrId` (current text form `<serial_hex>@<realm_id>/<publisher>/<program>@<version>`; the target ActorId form `<serial>.<realm_id>@<domain>` is specified in `actr/protocols/uri.zh.md` §5) and `actr://` URI handling. The spec defines `actr://` as a link to a Realm or an Actor in it, `actr://<domain>/<realm_id>[/path]` or `actr://<domain>/<serial>.<realm_id>[/path]`; the current `ActrUri` still parses `actr://<authority>/<target>` with an opaque target.
- **Authority Bootstrap Types**: Defines the public authority descriptor and ICE-configuration response shared by Hosts and Loran components.
- **Pure Data Layer**: This module is a pure data definition and utility layer without any high-level business logic, runtime implementation, or application framework trait definitions.

## Core Protocols

- **`webrtc.proto`**: Defines WebRTC-compatible base negotiation messages (`IceCandidate`, `SessionDescription`).
- **`actr.proto`**: Defines framework core business objects, including identity models (`ActrId`, `VTN`), service contracts (`ServiceSpec`), access control (`AclRule`), and core interactions.
- **`signaling.proto`**: Defines the top-level envelope `SignalingEnvelope` for all signaling server interactions.

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
actr-protocol = "0.10.0-alpha.4"
```

## License

Licensed under the Apache-2.0 license.
