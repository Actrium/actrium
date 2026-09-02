# Actr Package Resolution

Language: **English** | [简体中文](package-resolution.zh.md)

Status: initial `actr-packages/1` draft.

This document defines how an opaque Target in an `actr:` URI resolves to an
immutable `.actr` entry package. It also defines the HTTP wire format used by
the Package service advertised through Actr Authority Discovery.

## 1. Role and boundary

A non-empty Target identifies an **entry Actor Package** selected by its
authority. It does not identify a running Actor instance, a signaling endpoint,
or a session.

```text
actr://authority/opaque-target
        → GET /.well-known/actr
        → select rel=packages, protocol=actr-packages/1
        → resolve the complete Target
        → download an immutable .actr artifact
        → verify and start it in a compatible Host
```

Runtime Actor discovery happens only after the entry package has started. The
Package service and runtime discovery service are distinct protocol roles even
when one Loran deployment implements both.

## 2. Package service discovery

The authority descriptor advertises the Package service as a service link:

```json
{
  "rel": "packages",
  "href": "https://packages.example.com/actr/packages",
  "protocol": "actr-packages/1"
}
```

The `href` is a service endpoint, not a URL prefix for Targets. Clients must use
the request defined below and must not append Target path segments directly.

## 3. Target resolution request

An `actr-packages/1` service must support:

```http
GET /actr/packages/resolve?target=apps%2Fecho HTTP/1.1
Host: packages.example.com
Accept: application/json
```

`target` is required exactly once. Its value is the complete normalized Target
without the leading slash, encoded as an HTTP query value. The server treats it
as opaque and must not assume that its segments are a manufacturer, version,
realm, filesystem path, or Actor identity.

Resolution is safe and idempotent. Servers should provide explicit HTTP cache
controls. Public Targets may be resolved without credentials. Private Targets
may use normal HTTP authentication and authorization; secrets never belong in
the `actr:` URI.

## 4. Successful response

Success uses `200 OK` and `application/json`. The body conforms to
[`package-resolution.schema.json`](../../contracts/actr/package-resolution.schema.json):

```json
{
  "version": 1,
  "target": "apps/echo",
  "package": {
    "href": "https://packages.example.com/actr/packages/artifact?target=apps%2Fecho&sha256=0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    "media_type": "application/zip",
    "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    "size": 18432,
    "profile": "keel.guest/1"
  }
}
```

- `version` is the response format version and is `1` in this draft.
- `target` must exactly equal the submitted decoded Target.
- `href` is the absolute locator for the immutable artifact selected by this
  resolution.
- `media_type` is `application/zip` because `actr-package/1` is a ZIP container.
- `sha256` covers the complete response body of the artifact download.
- `size` is the expected number of artifact octets.
- `profile` is an optional hint. It can support an early compatibility check,
  but the signed package manifest remains authoritative.

Clients must ignore unknown response members and reject unsupported versions,
Target mismatches, malformed URLs, invalid digests, or invalid sizes.

## 5. Artifact download

The client retrieves the absolute `package.href` with `GET` and an appropriate
`Accept` header. A successful response uses `200 OK` and `application/zip`.

The client must limit the download, verify both `size` and `sha256`, and only
then pass the bytes to the Actor Package verifier. Transport integrity does not
replace manifest signature, publisher trust, Workload Profile, Host API, or
capability-policy verification.

Artifact URLs should be content-addressed and may therefore use long-lived
immutable caching. Loran includes the selected digest in its download URL and
rejects a request when that digest no longer matches the configured artifact.

## 6. Errors

- `400 Bad Request`: missing, repeated, or malformed Target input.
- `401 Unauthorized` / `403 Forbidden`: authentication or authorization is
  required or denied.
- `404 Not Found`: the authority does not publish the Target, or an immutable
  artifact reference no longer exists.
- `406 Not Acceptable`: no supported response representation.
- `503 Service Unavailable`: configured artifact storage is temporarily
  unavailable.

Error bodies are not successful Package Resolution documents.

## 7. Host behavior

A resolved Target can still be incompatible with the current Host. For example,
Actrium may accept `keel.guest/1`, while a Wasmtime Host accepts
`actr.component/1`. An incompatible Host reports an unsupported profile instead
of interpreting a server workload as a browsable page.

For Echo, the intended sequence is:

```text
open actr://example.com/echo
  → resolve and start echo-ui.actr
  → read its declared Echo dependency and contract
  → runtime discovery selects a running Echo Server Actor
  → signaling establishes the Actor connection
```

There is no need for a user-facing `actr://example.com/echo/server` URI.

## 8. Loran composition

Loran's `loran-packages` component owns Target resolution and artifact download.
It can run without the local Loran process serving `/.well-known/actr`. The
default Loran daemon may mount both and advertise the component through
`rel=packages`; an authority may instead advertise a different Loran node,
object-store gateway, or CDN-backed implementation.

MFR remains responsible for publisher identity, keys, signing authority, and
publication audit. It is not the artifact store merely because both components
can be deployed together.

## 9. Security considerations

- Production resolution and downloads use HTTPS. Development implementations
  may allow plaintext HTTP only for explicit loopback authorities.
- The resolver must not translate an untrusted Target into an unchecked local
  path. Loran resolves only exact administrator-configured Target mappings.
- Clients must apply redirect, response-size, timeout, and network-address
  policy before following artifact URLs.
- Cache keys must include authorization state for private Targets.
- An `actr:` URI and public resolution response contain no bearer secrets.

## 10. Related specifications

- [Actr URI Scheme](uri.md)
- [Actr Authority Discovery](authority-discovery.md)
- [Actr Actor Package specification](actor-package.zh.md)
