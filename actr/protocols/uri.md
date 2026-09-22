# Actr URI Scheme

Language: **English** | [简体中文](uri.zh.md)

Status: initial draft; not yet registered with IANA.

This document defines the syntax, normalization, and resolution model of the
`actr:` URI scheme. It is intended to become the specification referenced by
the IANA `actr` URI scheme registration.

## 1. Purpose and scope

An `actr:` URI is a stable logical reference to an Actr authority or to an
application entry published by that authority. It is both:

- an identifier that applications can store, compare, and exchange; and
- a bootstrap reference from which an Actr Host can resolve, verify, and start
  an entry Actor Package.

An `actr:` URI is not a direct signaling, WebSocket, WebRTC, STUN, or TURN
transport address. The authority delegates those network locations through
[Actr Authority Discovery](authority-discovery.md).

This document does not assign fixed path positions to an Actr realm,
manufacturer, Actor type, version, or serial number. The target reference stays
opaque and resolves to an entry Actor Package release through
[Actr Package Resolution](package-resolution.md).

An `actr:` URI does not address a running Actor instance. Instance discovery,
routing, signaling, and session establishment after startup belong to the Actr
runtime protocols.

The keywords **MUST**, **MUST NOT**, **SHOULD**, **SHOULD NOT**, and **MAY** are
to be interpreted as described by BCP 14 when they appear in uppercase.

## 2. Syntax

The `actr` scheme uses the generic hierarchical URI syntax from RFC 3986. Its
initial grammar is:

```abnf
actr-uri        = "actr://" actr-authority [ "/" target-reference ]
actr-authority  = host [ ":" port ]
target-reference = target-segment *( "/" target-segment )
target-segment  = 1*( unreserved / pct-encoded / sub-delims / ":" / "@" )
```

`host`, `port`, `unreserved`, `pct-encoded`, and `sub-delims` are imported
from RFC 3986. Although URI scheme names are case-insensitive, canonical Actr
URIs use the lowercase scheme `actr`.

The following generic URI features are deliberately excluded in this draft:

- `userinfo` in the authority;
- query components;
- fragment components;
- empty target segments; and
- the dot segments `.` and `..`.

## 3. URI components

### 3.1 Scheme

The scheme name is `actr`. It identifies the logical Actr addressing and
bootstrap model defined by this specification. It does not select an
underlying transport by itself.

### 3.2 Authority

The URI authority is a DNS name or IP literal with an optional port:

```text
actr://example.com
actr://example.com:8443
actr://[2001:db8::1]:8443
```

The authority is the stable administrative and trust boundary for the Actr
target. It is also the HTTPS origin used for authority discovery. If the port
is omitted, authority discovery uses the default HTTPS port. If present, the
explicit port is used by the discovery HTTPS origin.

A Realm is not part of the URI authority. A port-less DNS authority can also
act as a Realm namespace: the domain supplies a globally unique,
ownership-verifiable name, and each Realm under it is one application instance
identified by a local name (Section 7.1). One authority can therefore be the
namespace of many Realms without operating any Actr infrastructure itself.

The `userinfo` production from generic URI syntax is forbidden. Therefore an
Actr URI never uses a form such as `actr://user@example.com/...`.

### 3.3 Target reference

The optional path after the authority is the target reference:

```text
actr://example.com/echo
actr://example.com/actors/echo
```

These examples illustrate path shape only; the segment names have no meaning
assigned by this draft.

An empty target reference identifies the authority itself. A non-empty target
reference is interpreted by the authority's Package service and resolves to an
entry Actor Package release. Generic Actr URI parsers **MUST** preserve its
normalized segment sequence and **MUST NOT** infer realm, manufacturer,
version, Actor identity, or repository paths from fixed path positions.

A target reference **MUST NOT** resolve to a running Actor instance, signaling
endpoint, or current session. An authority also **MUST NOT** require clients to
append the target directly to the Package service URL; Actr Package Resolution
defines the request format.

Target segments are case-sensitive. Arbitrary octets and characters outside
the permitted ASCII set must be represented with UTF-8 followed by
percent-encoding as defined by RFC 3986.

### 3.4 Query and fragment

Query and fragment components are not defined in this draft and **MUST NOT**
appear in a conforming Actr URI. Protocol extensions must not use them to carry
credentials, access tokens, private keys, or other secrets.

## 4. Resolution model

Given an `actr:` URI, a client performs these logical steps:

1. Parse and normalize the URI.
2. Extract the `host[:port]` authority.
3. Retrieve `https://<authority>/.well-known/actr` as defined by Actr Authority
   Discovery.
4. Verify that the returned descriptor is bound to the original authority.
5. If the URI contains a target reference, select the descriptor's
   `rel=packages` service.
6. Submit the complete target according to Actr Package Resolution and obtain
   an immutable Package release reference.
7. Download the root `.actr` artifact and verify its identity, version, digest,
   publisher signature, Workload Profile, and Host requirements.
8. Install and start it only when the Host and local policy allow it.
9. After startup, use manifest dependencies, contracts, and runtime discovery
   to locate and connect to running Actor instances.

The `actr:` URI therefore remains stable when an authority moves signaling or
other infrastructure to a different network endpoint.

## 5. Normalization and comparison

Before serialization or comparison, an Actr URI is normalized as follows:

1. Serialize the scheme as lowercase `actr`.
2. Normalize the host according to RFC 3986. DNS names are lowercase.
   Internationalized DNS names are serialized in their IDNA ASCII form.
3. Remove an explicit port `443`, because it is the default discovery HTTPS
   port.
4. Use uppercase hexadecimal digits in percent-encoded triplets.
5. Decode percent-encoded octets that represent unreserved characters.
6. Reject empty target segments and the dot segments `.` and `..` rather than
   resolving them.
7. Omit the trailing `/` when the URI identifies the authority and has no
   target reference.

IPv6 literals **SHOULD** use the RFC 5952 canonical text form.

Two Actr URIs are equivalent when their normalized serialized forms are
identical. Target references remain case-sensitive; implementations **MUST
NOT** lowercase them during comparison.

## 6. Examples

Conforming examples:

| URI | Meaning at this layer |
| --- | --- |
| `actr://example.com` | The Actr authority `example.com`. |
| `actr://example.com/echo` | The authority's Echo entry Actor Package Target. |
| `actr://example.com/actors/echo` | An opaque, multi-segment Package Target. |
| `actr://example.com:8443/echo` | Authority discovery uses HTTPS port 8443. |
| `actr://[2001:db8::1]:8443/echo` | An authority expressed as an IPv6 literal. |

Non-conforming examples:

| URI | Reason |
| --- | --- |
| `actr://user@example.com/echo` | User information is forbidden. |
| `actr://example.com/echo?token=secret` | Query components are undefined and must not carry credentials. |
| `actr://example.com/echo#method` | Fragment components are undefined. |
| `actr://example.com/a//b` | Empty target segment. |
| `actr://example.com/a/../b` | Dot segment. |
| `actr://101:acme+echo@1.0.0` | Legacy syntax; not an RFC 3986 Actr authority. |

## 7. Relationship to Actor Packages and instances

An Actr runtime may use structured identities containing a Realm reference,
serial number, publisher, Actor type, and version. Those identities are not
Actr URIs and are never placed in the URI authority.

A non-empty target resolves only to an entry Actor Package release. The Target
may remain stable while release policy advances it, but each resolution result
binds an exact immutable artifact digest and locator.

The Host creates an Actor instance from the verified Package and deployment
configuration. That Actor later discovers dependencies through its manifest,
contracts, and runtime discovery. A browser user can therefore open
`actr://example.com/echo` to start an Echo UI without navigating to a URI for a
particular Echo Server instance.

### 7.1 Realm references

A Realm is the trust and governance domain of one application instance. It is
named by a namespace and a local name:

```abnf
realm-ref       = local-name "~" realm-namespace
realm-namespace = "dns:" dns-name / "local:" installation-id
local-name      = let-dig [ *61( let-dig / "-" ) let-dig ]
let-dig         = %x61-7A / DIGIT              ; lowercase a-z and 0-9
installation-id = 26( %x61-7A / %x32-37 )      ; lowercase base32, a-z and 2-7
```

- `dns-name` is a DNS name normalized as in Section 5: lowercase IDNA A-labels
  with no port, userinfo, or path. `dns:<dns-name>` is the same namespace as
  the port-less Actr authority `actr://<dns-name>`; its namespace keys are
  published by [Actr Authority Discovery](authority-discovery.md). An
  authority with an explicit port or an IP literal is never a Realm namespace.
- `local:<installation-id>` is the namespace of one Actr Host installation.
  `installation-id` is derived from the installation root key as defined by
  [Actr Realm Profile](realm-profile.md). A local Realm reference is usable
  only on that installation.
- `local-name` follows DNS label rules: 1 to 63 characters, lowercase ASCII
  letters, digits, and `-`, not starting or ending with `-`. It is unique
  within its namespace, immutable after creation, and never reused after the
  Realm is retired. Hosts generate a random local name by default
  (Section 9); a readable name is an explicit choice.

Examples:

```text
k7m2q9xh~dns:acme.com                ; default random local name
notes~dns:acme.com                   ; readable name chosen explicitly
h3x8p2wq~local:<installation-id>
```

Realm references are compared as exact strings after normalization. Parsing a
Realm reference is purely syntactic and has no network side effect; resolving
a DNS Realm reference to a verified Realm Profile is defined by Actr Realm
Profile.

### 7.2 Actor identity text form

The text form of a persistent Actor identity is:

```text
<serial_hex>@<realm-ref>/<publisher>:<program>:<version>
```

For example, `2af01@k7m2q9xh~dns:acme.com/notes.example:team-notes:1.4.2`.
`serial_hex` is the lowercase hexadecimal serial number issued by the Realm's
issuer. Provider-internal numeric realm identifiers **MUST NOT** appear in
this form. An Actor identity is not an Actr URI; when one is carried inside a
URI, the carrying protocol percent-encodes it as data.

## 8. Legacy URI migration

The existing Actr prototype has accepted this historical form:

```text
actr://<realm>:<manufacturer>+<name>@<version>
```

For example:

```text
actr://101:acme+echo-service@1.0.0
```

This form places application fields where RFC 3986 expects a network
authority. Because `@` separates user information from the host, a generic URI
parser interprets `101:acme+echo-service` as user information and `1.0.0` as
the host. It is therefore not conforming to this specification.

Implementations **MAY** recognize the historical form only in an explicit
legacy migration mode. They **MUST NOT** silently treat it as a conforming new
Actr URI. Migration tooling must resolve the legacy realm and Actor type
through configured legacy authority data, then emit a new authority-based URI.

## 9. Security and privacy considerations

- An Actr URI is likely to appear in logs, browser history, crash reports,
  telemetry, clipboards, and inter-application messages. It must not contain
  credentials or secret authorization material.
- Clients must validate HTTPS and the authority binding before trusting
  discovered service endpoints.
- Parsers must reject invalid percent-encoding, control characters, malformed
  authorities, forbidden components, empty path segments, and dot segments.
- Clients must apply scheme and network policy to discovered endpoints to
  prevent downgrade and server-side request forgery attacks.
- Display software should make the authority visually distinct and account for
  internationalized-domain homograph risks.
- Authorization is evaluated when resolving or contacting a target. Possession
  of an Actr URI alone does not grant access.
- Realm references and Actor identities reveal the Realm namespace and local
  name wherever they are shared. Hosts **SHOULD** generate a random local name
  by default when creating a Realm and keep the human-readable title in the
  Realm Profile's `display_name`.

## 10. IANA considerations

The intended provisional registration is:

```text
Scheme name: actr
Status: Provisional
Applications/protocols that use this scheme name: Actr
Contact: Actr Protocol Maintainers
Change controller: Actrium Project
References: this document
```

The `actr` scheme supports authority discovery through the HTTPS Well-Known URI
defined by Actr Authority Discovery.

## 11. References

- [RFC 3986: Uniform Resource Identifier: Generic Syntax](https://www.rfc-editor.org/rfc/rfc3986.html)
- [RFC 5952: A Recommendation for IPv6 Address Text Representation](https://www.rfc-editor.org/rfc/rfc5952.html)
- [RFC 7595: Guidelines and Registration Procedures for URI Schemes](https://www.rfc-editor.org/rfc/rfc7595.html)
- [RFC 8615: Well-Known Uniform Resource Identifiers](https://www.rfc-editor.org/rfc/rfc8615.html)

## 12. Open work

This initial draft deliberately leaves the following work open:

- define the target-reference selectors and resolution protocol;
- define how public target references map to Realms and structured Actor
  identities;
- freeze the binary encoding of Actor identities that carry a Realm reference;
- specify error behavior and authorization challenges during target
  resolution;
- add parser, normalization, equivalence, and legacy-migration test vectors;
- establish the durable public specification URL and project contact used by
  the IANA registration.
