# actr-protocol

Unified protocol, types, and URI parsing for Actor-RTC framework.

## Overview

`actr-protocol` is the protocol foundation layer of the Actor-RTC framework. It defines the standardized underlying communication contracts required for the framework's operation and provides stateless utility functions closely related to these protocols.

## Features

- **Protocol Definitions**: Provides `.proto` files and generated Rust types covering identity, signaling, service discovery, and other framework-level concepts.
- **Core Utilities**: Provides necessary extensions for core types, such as string parsing/formatting for `ActrId` (ActorId `<serial_hex>.<realm_id>@<domain>`, see `actr/protocols/uri.zh.md` §5.2; the Actor type is carried by the Actor credential, not the ActorId), Realm name validation (`uri::parse_realm_name`), and `actr://` URI handling: `actr://<domain>/<realm_id>[/path]` links to a Realm and `actr://<domain>/<serial>.<realm_id>[/path]` to an Actor in it.
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
