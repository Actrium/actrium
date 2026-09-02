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

Version 1 defines no additional path components below `/.well-known/actr`.
Per-realm discovery variations are also outside the scope of version 1. A
deployment that requires independently controlled discovery documents should
use distinct Actr authorities, normally distinct hostnames.

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
| `identity` | Resolve identities, keys, and authentication metadata. |
| `discovery` | Resolve Actor targets and capabilities. |
| `ice-config` | Obtain ICE server configuration and, after authorization, short-lived TURN credentials as defined by [Actr ICE Configuration](ice-configuration.md). |
| `contracts` | Obtain public Actor contract and interface descriptions. |
| `packages` | Resolve a public URI Target to an entry Actor Package and retrieve its `.actr` artifact. |

Extension relation values **MUST** be absolute URIs controlled by the extension
author. This avoids collisions with short core relation names.

The descriptor **MUST NOT** contain access tokens, passwords, long-lived TURN
credentials, private keys, or user-specific data. In particular,
`ice-config` identifies an endpoint; it does not embed private TURN
credentials in this public document.

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
relations, or clarification without changing `version`. A revision that
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
- add interoperability fixtures and negative test cases;
- establish the durable public specification URL and project contact used by
  the IANA registration.
