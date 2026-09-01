# Actr URI Scheme

Language: **English** | [简体中文](uri.zh.md)

Status: initial draft; not yet registered with IANA.

This document defines the syntax, normalization, and resolution model of the
`actr:` URI scheme. It is intended to become the specification referenced by
the IANA `actr` URI scheme registration.

## 1. Purpose and scope

An `actr:` URI is a stable logical reference to an Actr authority or to a
target organized by that authority. It is both:

- an identifier that applications can store, compare, and exchange; and
- a bootstrap reference from which an Actr client can discover the services
  needed to resolve and contact the target.

An `actr:` URI is not a direct signaling, WebSocket, WebRTC, STUN, or TURN
transport address. The authority delegates those network locations through
[Actr Authority Discovery](authority-discovery.md).

This document does not yet standardize the internal structure or resolution
semantics of the target reference. In particular, it does not assign fixed
path positions to an Actr realm, manufacturer, Actor type, version, or serial
number. Those semantics require a separate target-resolution specification.

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

An Actr realm is an application-level namespace and is not part of the URI
authority. One authority can operate multiple realms under the same domain and
TLS control. Realms that require independent bootstrap control should use
distinct authorities, normally distinct hostnames.

The `userinfo` production from generic URI syntax is forbidden. Therefore an
Actr URI never uses a form such as `actr://user@example.com/...`.

### 3.3 Target reference

The optional path after the authority is the target reference:

```text
actr://example.com/echo
actr://example.com/actors/echo
actr://example.com/actors/echo/instances/1a2b3c
```

These examples illustrate path shape only; the segment names have no meaning
assigned by this draft.

An empty target reference identifies the authority itself. A non-empty target
reference is interpreted by the target-resolution protocol advertised by that
authority. Generic Actr URI parsers **MUST** preserve its normalized segment
sequence and **MUST NOT** infer realm, manufacturer, version, or Actor identity
fields from fixed path positions.

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
5. Select the advertised discovery, identity, signaling, and ICE configuration
   services needed for the operation.
6. If the URI contains a target reference, submit that reference to the
   authority's target-resolution service.
7. Authenticate and authorize the caller through the selected services before
   disclosing private target information or establishing a session.

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
| `actr://example.com/echo` | Target reference `echo` under that authority. |
| `actr://example.com/actors/echo` | An opaque, multi-segment target reference. |
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

## 7. Relationship to Actor identities

An Actr runtime may use structured identities containing a realm, serial
number, manufacturer, Actor type, and version. Those internal identities are
not automatically public Actr URIs.

A target-resolution service may resolve an opaque target reference to:

- an exact Actor instance;
- an Actor type or version range;
- an application or service group; or
- another authority-defined target.

The future target-resolution specification must define those selectors and
their mapping to structured Actor identities. Implementations must not expose
an internal database key or storage-oriented string format as a public URI
without an explicit stable mapping.

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
- define how public target references map to realms and structured Actor
  identities;
- specify error behavior and authorization challenges during target
  resolution;
- add parser, normalization, equivalence, and legacy-migration test vectors;
- establish the durable public specification URL and project contact used by
  the IANA registration.
