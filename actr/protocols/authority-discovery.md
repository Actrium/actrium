# Actr Authority Discovery

Language: **English** | [简体中文](authority-discovery.zh.md)

Status: initial draft; not yet registered with IANA.

This document defines how an Actr client discovers the public services for an
Actr authority. It is intended to become the specification referenced by the
IANA `actr` Well-Known URI registration.

## 1. Scope

An `actr:` URI identifies an Actr target under an authority. The authority is
the stable name that organizes the target; it is not necessarily the network
location of the signaling, identity, discovery, or ICE services.

Authority discovery maps that stable authority to service endpoints. It does
not define the `actr:` URI target syntax, Actor resolution, signaling protocol,
authentication protocol, or ICE configuration response. Those are separate
Actr protocol concerns. The URI syntax is defined by the
[Actr URI specification](uri.md).

The keywords **MUST**, **MUST NOT**, **SHOULD**, **SHOULD NOT**, and **MAY** are
to be interpreted as described by BCP 14 when they appear in uppercase.

## 2. Discovery URI

For an `actr:` URI whose authority is `example.com`, the discovery resource is:

```text
https://example.com/.well-known/actr
```

If the Actr authority explicitly contains a port, the same port is used by the
HTTPS origin. If it does not contain a port, the default HTTPS port is used.

The discovery document applies only to the exact HTTPS origin from which it is
obtained. It does not automatically apply to parent domains, subdomains, or
other ports.

Version 1 defines no additional path components below `/.well-known/actr`,
and no specification may assign meaning to such subpaths. Information that is
specific to one Realm or to one publisher is not discovered through subpaths.
Instead, the descriptor MAY carry the signed members defined in Section 4.4
and Section 4.5, and the `realms` relation (Section 4.3) locates the Realm
Profiles defined by [Actr Realm Profile](realm-profile.md). One authority can
therefore serve as the namespace of many Realms. A deployment that requires
independently controlled discovery documents should use distinct Actr
authorities, normally distinct hostnames.

## 3. Request

Clients **MUST** retrieve the resource with HTTPS and validate the server
certificate according to normal HTTPS rules.

Servers **MUST** support:

```http
GET /.well-known/actr HTTP/1.1
Host: example.com
Accept: application/json
```

The request is public bootstrap discovery. Clients **MUST NOT** send an
`Authorization` header, cookies, or other application credentials. Servers
**SHOULD** support `HEAD` with the same response headers as `GET` and no
response body. No semantics are defined for other methods.

## 4. Successful response

A successful response **MUST** use status `200 OK` and media type
`application/json`. The response body **MUST** be a UTF-8 JSON object conforming
to
[`authority-descriptor.schema.json`](../../contracts/actr/authority-descriptor.schema.json).

Example:

```json
{
  "version": 1,
  "authority": "actr://example.com",
  "services": [
    {
      "rel": "signaling",
      "href": "wss://signal.example.com/actr",
      "protocol": "actr-signaling/1"
    },
    {
      "rel": "identity",
      "href": "https://identity.example.com/actr"
    },
    {
      "rel": "discovery",
      "href": "https://directory.example.com/actr"
    },
    {
      "rel": "ice-config",
      "href": "https://rtc.example.com/actr/ice",
      "protocol": "actr-ice-config/1"
    },
    {
      "rel": "contracts",
      "href": "https://contracts.example.com/actr"
    },
    {
      "rel": "packages",
      "href": "https://packages.example.com/actr/packages",
      "protocol": "actr-packages/1"
    }
  ]
}
```

JSON member names **MUST** be unique. Clients **MUST** reject a document with
duplicate member names, a missing required member, or an unsupported
`version`. Clients **MUST** ignore unknown members so compatible additions do
not require a new discovery URI.

### 4.1 `version`

`version` identifies the authority descriptor format. This draft defines
integer version `1`. Descriptor format evolution belongs in this member, not
in a new Well-Known URI suffix such as `actr-v2`.

### 4.2 `authority`

`authority` is the canonical Actr authority URI described by the document. It
contains the `actr` scheme and authority, but no target path, query, fragment,
or user information.

After applying the normalization rules from the Actr URI specification, a
client **MUST** verify that this value matches the authority used to construct
the discovery request. A redirect does not change this binding.

### 4.3 `services`

`services` is a non-empty array of service links. More than one entry can use
the same relation when an authority advertises multiple endpoints.

Each service link contains:

- `rel`: the role of the service.
- `href`: an absolute URI for the service endpoint.
- `protocol`: an optional Actr protocol name and major version supported by
  the endpoint.

Version 1 defines these core relations:

| Relation | Purpose |
| --- | --- |
| `signaling` | Establish Actor connectivity and exchange session negotiation messages. |
| `identity` | Resolve identities, keys, and authentication metadata. In a provider-owned Realm namespace, its `href` is also the OAuth 2.0 issuer for account sign-in ([Actr Realm Profile](realm-profile.md), Section 7.3). |
| `discovery` | Resolve Actor targets and capabilities. |
| `ice-config` | Obtain ICE server configuration and, after authorization, short-lived TURN credentials as defined by [Actr ICE Configuration](ice-configuration.md). |
| `contracts` | Obtain public Actor contract and interface descriptions. |
| `packages` | Resolve a public URI Target to an entry Actor Package and retrieve its `.actr` artifact. |
| `realms` | Retrieve signed Realm Profiles for Realms in this authority's namespace, as defined by [Actr Realm Profile](realm-profile.md). The protocol value is `actr-realm-profile/1`. |
| `realm-management` | Accept provider management requests (`POST`) for Realms this authority serves as a provider, as defined by [Actr Realm Profile](realm-profile.md), Section 8. The protocol value is `actr-realm-management/1`. |

Extension relation values **MUST** be absolute URIs controlled by the extension
author. This avoids collisions with short core relation names.

The descriptor **MUST NOT** contain access tokens, passwords, long-lived TURN
credentials, private keys, or user-specific data. In particular,
`ice-config` identifies an endpoint; it does not embed private TURN
credentials in this public document.

When the descriptor carries `realm_namespace` or `realms`, its `signaling`,
`identity` and `ice-config` links serve the authority itself and, at most,
suggest a provider for creating new Realms. They never serve a Realm in this
namespace: a client **MUST** reach an existing Realm only through the provider
named by that Realm's valid Profile, and **MUST NOT** fall back to these links
when the Profile is unavailable, invalid, or retired.

### 4.4 `realm_namespace`

`realm_namespace` is an optional member. When present, the authority's DNS
name, without a port, is a Realm namespace written `dns:<host>`. An authority
with an explicit port or an IP-literal host **MUST NOT** carry this member, and
clients **MUST** ignore it in that case.

The member publishes the namespace keys, which are the long-term trust root of
every Realm in the namespace:

```json
"realm_namespace": {
  "sequence": 3,
  "keys": [
    { "id": "ns-2026-1", "algorithm": "Ed25519", "public_key": "<base64>", "status": "active" },
    { "id": "ns-2025-1", "algorithm": "Ed25519", "public_key": "<base64>", "status": "retired" }
  ],
  "rotations": [
    { "from": "ns-2025-1", "to": "ns-2026-1", "sequence": 2, "signature": "<base64>" }
  ],
  "signature": { "key_id": "ns-2026-1", "algorithm": "Ed25519", "value": "<base64>" }
}
```

- `sequence` is a non-negative integer that increases with every change to the
  member.
- `keys` lists the namespace keys. Each key has a unique `id`, `algorithm`
  (version 1 defines only `Ed25519`), a base64 `public_key`, and `status`. An
  `active` key signs; a `retired` key no longer signs, but signatures it made
  earlier, such as on existing Realm Profiles, stay valid. Because signatures
  carry no trusted time, `retired` gives no security guarantee; revoking a key
  requires removing it. Removing a key from
  `keys` revokes it: nothing it signed is accepted any longer, so a namespace
  owner removes a key that may be compromised and re-signs the affected
  Profiles. At least one key **MUST** be `active`.
- `rotations` lists the rotation chain. In each entry, the key named by `from`
  signs the JCS form of
  `{"from": ..., "to": ..., "public_key": ..., "sequence": ...}`, where
  `public_key` is that of the `to` key in `keys` and `sequence` is the member
  `sequence` that introduced it, and
  `signature` carries that detached signature.
- `signature` is a detached Ed25519 signature by an `active` key over the
  RFC 8785 JCS canonical form of the member with `signature` removed. It
  covers `sequence`.

The first acceptance of a namespace relies on HTTPS (Web PKI) or on a namespace
key pinned by enterprise policy. After that, a client or provider that has
pinned the namespace treats every key it has ever accepted as pinned, and:

- **MUST** reject a member whose `signature` does not verify;
- **MUST** ignore rotation entries whose `from` key is not in the current
  `keys`; such entries do not invalidate the member, and a verifiable chain is
  required only for keys not yet accepted;
- **MUST** accept a key it has not yet accepted only through a valid rotation
  chain from a pinned key, where each new rotation entry carries a `sequence`
  higher than the accepted member `sequence`;
- **MUST** reject a member or new rotation entry signed by a key that a
  previously accepted member already marked `retired` or had removed;
- **MUST** reject a member signed through the pinned chain whose `sequence` is
  lower than the accepted value;
- **MUST** treat a member whose signing key does not chain to a pinned key as
  an identity change: it performs no `sequence` comparison, requires the user
  or operator to confirm trust again, and after confirmation uses the new
  member's `sequence` as its baseline.

The member **MUST NOT** list the Realms in the namespace, so that Realm names
cannot be enumerated from the descriptor.

### 4.5 `publisher`

`publisher` is an optional member that carries the authority's signed
Publisher Profile inline. When present, the authority's DNS name, without a
port, is the Publisher ID. It replaces standalone publisher bootstrap files;
specifications **MUST NOT** define other Well-Known locations such as
`/.well-known/actr/publisher.json` or `/.well-known/actrium-publisher.json`
for this purpose.

The Publisher Profile uses snake_case member names and
`"schema": "actr.publisher/v1"`. Its full format, key roles, and rotation
rules are left to the Actor Package specification (Section 10). At the
descriptor level:

- `domain` **MUST** equal the host of `authority`;
- `program_resolver` is required and names the HTTPS base URL for program
  release metadata; there is no default location;
- the Profile is signed by an active profile signing key and verified by its
  own signature, independently of the HTTPS channel;
- HTTPS (or an enterprise pin) provides only the first-contact binding between
  the domain and the publisher's keys;
- after the first Profile is accepted, sequence and rotation checks follow the
  Actor Package specification, not the descriptor's HTTP caching.

A provider that registers publishers does so through a `register_publisher`
request that reuses the provider management envelope of
[Actr Realm Profile](realm-profile.md), Section 8, signed by an active
profile signing key, not through a challenge placed in the public descriptor.

### 4.6 Signed and unsigned parts

The descriptor as a whole remains an unsigned public document protected by
HTTPS. `realm_namespace` and `publisher` are verified only by their own
signatures, and a valid signature on one of them does not authenticate any
other member. The document size limit in Section 6 applies to the whole
descriptor, including signed members.

## 5. Errors, redirects, caching, and CORS

An origin that does not support Actr discovery **SHOULD** return `404 Not
Found`. Other failures use their normal HTTP status codes. Error response
bodies are not authority descriptors.

Servers **SHOULD** serve the discovery resource directly. A client **MAY**
follow at most five redirects, but every redirect target **MUST** use HTTPS. A
client **MUST NOT** treat the redirect target as a new Actr authority and
**MUST** still validate the returned `authority` against the original Actr
authority.

Servers **SHOULD** send explicit `Cache-Control` freshness information. Clients
and intermediaries **MUST** apply normal HTTP caching semantics. This draft
does not define an implicit freshness lifetime when the server omits one.

Because the descriptor is public and contains no credentials, deployments
that support browser-based Actr clients **SHOULD** send:

```http
Access-Control-Allow-Origin: *
```

Credentialed cross-origin discovery is not defined by this specification.

## 6. Security and privacy considerations

- HTTPS is mandatory. Clients **MUST NOT** downgrade discovery or redirected
  requests to plaintext HTTP.
- Control of the discovery document represents control of the entire Actr
  authority. Write access to it must be restricted accordingly.
- Clients must validate the authority binding before using any advertised
  service.
- Clients must validate each service URI and allow only schemes supported for
  that relation. Implementations should also guard against server-side request
  forgery when a service URI is consumed by a privileged process.
- The descriptor is public, predictable, and cacheable. It must not contain
  secrets, user identifiers, or authorization state.
- Implementations should limit response size and JSON parsing resources. A
  default maximum document size of 64 KiB is recommended.

## 7. Versioning and compatibility

Compatible revisions may add optional members, new URI-valued extension
relations, or clarification without changing `version`. The `realms` relation
and the `realm_namespace` and `publisher` members are such additions: clients
that do not implement them ignore them. A revision that
changes the meaning of an existing member or makes a previously valid
descriptor invalid requires a new descriptor version.

The Well-Known URI remains `/.well-known/actr` across descriptor versions.

## 8. IANA considerations

The intended provisional registration is:

```text
URI suffix: actr
Change controller: Actrium Project
Specification document: this document
Related information: none
```

The permitted URI scheme for the Well-Known URI is `https`.

This document uses the already registered `application/json` media type and
does not request a new media type. The JSON Schema is part of the Actr
specification and does not require a separate IANA registration.

## 9. References

- [RFC 3986: Uniform Resource Identifier: Generic Syntax](https://www.rfc-editor.org/rfc/rfc3986.html)
- [RFC 8259: The JavaScript Object Notation Data Interchange Format](https://www.rfc-editor.org/rfc/rfc8259.html)
- [RFC 8615: Well-Known Uniform Resource Identifiers](https://www.rfc-editor.org/rfc/rfc8615.html)
- [RFC 9110: HTTP Semantics](https://www.rfc-editor.org/rfc/rfc9110.html)
- [RFC 9111: HTTP Caching](https://www.rfc-editor.org/rfc/rfc9111.html)

## 10. Open work

This initial draft deliberately leaves the following work open:

- stabilize the Actr URI specification and its authority mapping;
- define the individual service protocols and their allowed URI schemes;
- decide whether service ordering, priority, or failover metadata is needed;
- specify the `actr.publisher/v1` Publisher Profile format in the Actor
  Package specification;
- define the rotation-chain payload of `realm_namespace` in a machine-readable
  schema and add signing test vectors;
- define delegated Realm-creation keys, which version 1 does not provide;
- add interoperability fixtures and negative test cases;
- establish the durable public specification URL and project contact used by
  the IANA registration.
