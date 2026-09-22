# Actr Realm Profile

Language: **English** | [简体中文](realm-profile.zh.md)

Status: initial draft.

This document defines how an Actr client or provider resolves a Realm
reference to a verified Realm Profile, the `actr.realm/v1` Profile format, and
the management requests a provider accepts.

## 1. Scope

A Realm is the trust and governance domain of one application instance.
Creating an application instance creates a new Realm. A Realm is named by a
namespace and a local name, written as a Realm reference such as
`k7m2q9xh~dns:acme.com`. The syntax of Realm references is defined in
[Actr URI](uri.md), Section 7.1.

A Realm Profile is a signed, public document that says which issuer and which
provider serve a Realm. The namespace key signs it, so the Profile can be
hosted anywhere: on the domain's own static hosting, at a provider, or
embedded in an invitation. Where it was fetched from does not affect its
validity.

A Realm is served by one provider for its whole life. A provider is a running
deployment, run by some operator, that provides the Realm's signaling, TURN,
and issuer. Moving an application to another operator means creating a new
Realm there and migrating application data; the old Realm is retired.

This document does not define the signaling, identity, or ICE protocols a
provider exposes, the Actor credential format, or membership and
authorization inside an application instance.

The keywords **MUST**, **MUST NOT**, **SHOULD**, **SHOULD NOT**, and **MAY** are
to be interpreted as described by BCP 14 when they appear in uppercase.

## 2. Namespaces

| Namespace | Owner | Trust root |
| --- | --- | --- |
| `dns:<domain>` | The domain owner | The namespace keys in the `realm_namespace` member of `https://<domain>/.well-known/actr` |
| `local:<installation-id>` | One Actr Host installation | The Host's own local records |

### 2.1 DNS namespaces

A DNS namespace is the same name as the port-less Actr authority
`actr://<domain>`. Its Ed25519 namespace keys are the long-term trust root of
every Realm in the namespace. Their publication, rotation, and rollback
protection are defined by [Actr Authority Discovery](authority-discovery.md),
Section 4.4. A domain publishes its namespace keys once; creating a Realm does
not change the domain's hosting.

A provider-owned namespace, such as `dns:realms.provider.example`, is an
ordinary DNS namespace whose keys the provider holds. It lets users without a
domain create Realms (Section 7.3).

### 2.2 Local namespaces

A local namespace belongs to one Host installation. Its `installation-id` is
the lowercase RFC 4648 base32 encoding, without padding, of the first 16 bytes
of the SHA-256 digest of the raw 32-byte installation root public key.

Local Realms have no Profile, no provider, and no management requests. The
Host keeps a local record for each local Realm. A local Realm reference is
usable only on that installation and cannot be resolved by other parties. The
rest of this document applies to DNS namespaces only.

## 3. Locating a Realm Profile

For a Realm reference `<local_name>~dns:<domain>`, a client:

1. obtains and verifies the authority descriptor of `actr://<domain>` and its
   `realm_namespace` member, as defined by Actr Authority Discovery;
2. selects the service links with `rel` `realms` and protocol
   `actr-realm-profile/1`;
3. for each link in document order, removes any trailing `/` from `href` and
   sends `GET <href>/<local_name>` with `Accept: application/json`, stopping
   at the first response that yields a valid Profile.

A `realms` href **MUST NOT** contain a query or fragment. It **MAY** point to
the domain's own hosting or to a provider, and **MAY** carry an explicit port.
If no link yields a valid Profile (because of `404 Not Found`, another error,
a timeout, or an invalid document), the Profile is unavailable.

The request is public. Clients **MUST** use HTTPS and **MUST NOT** send
`Authorization` headers, cookies, or other credentials. Redirects follow the
rules of Actr Authority Discovery: at most five, every hop over HTTPS. A
privileged process, such as a provider, **MUST** additionally apply
server-side request forgery checks on every hop: reject non-public addresses,
pin each hop to the pre-checked address, and limit the response to 64 KiB.

A Profile **MAY** also be delivered out of band, for example embedded in an
invitation. A client verifies such a Profile exactly as one fetched over
HTTPS.

## 4. Profile format

A Realm Profile is a UTF-8 JSON object. Member names use snake_case.

```json
{
  "schema": "actr.realm/v1",
  "sequence": 31,
  "realm": {
    "namespace": "dns:acme.com",
    "local_name": "k7m2q9xh",
    "display_name": "Acme Notes",
    "status": "active"
  },
  "issuer": {
    "id": "actr:realm:k7m2q9xh~dns:acme.com:authority",
    "url": "https://identity.provider.example/realms/acme.com/k7m2q9xh",
    "keys": [
      { "id": "issuer-2026-2", "algorithm": "Ed25519", "public_key": "<base64>", "status": "active" }
    ]
  },
  "provider": {
    "id": "provider.example",
    "signaling": [
      {
        "url": "wss://signal-a.provider.example/ws",
        "priority": 10,
        "control_plane_id": "provider.example/v1",
        "ticket_audience": "signal-a"
      }
    ],
    "ice": [
      { "urls": ["stun:stun.provider.example:3478"] },
      { "urls": ["turns:turn.provider.example:5349"], "credential_audience": "turn-pool-1" }
    ]
  },
  "policy": {
    "membership": "invite-only"
  },
  "issued_at": "2026-07-29T00:00:00Z",
  "signature": { "key_id": "ns-2026-1", "algorithm": "Ed25519", "value": "<base64>" }
}
```

| Member | Meaning |
| --- | --- |
| `schema` | **MUST** be `actr.realm/v1`. |
| `sequence` | Non-negative integer that increases with every Profile published for this Realm. |
| `realm.namespace`, `realm.local_name` | The Realm reference. They **MUST** match the reference being resolved. |
| `realm.display_name` | Human-readable title. It is not an identifier. |
| `realm.status` | `active` or `retired`. |
| `issuer.id` | **MUST** be `actr:realm:<realm-ref>:authority`. Actor credentials for the Realm carry this value as their issuer. |
| `issuer.url` | Registration and verification endpoint of the issuer. |
| `issuer.keys` | Issuer public keys used to verify Actor credentials. |
| `provider.id` | The provider's canonical DNS authority. It never changes during the Realm's life. |
| `provider.signaling` | Signaling endpoints. Entries sharing a `control_plane_id` are replicas or failover entries of one provider control plane, not a federation. |
| `provider.ice` | STUN and TURN endpoints. TURN entries name the `credential_audience` for short-lived TURN credentials. |
| `policy` | Join policy of the Realm. |
| `issued_at` | RFC 3339 time the Profile was signed. It is informational; Profiles do not expire. |
| `signature` | Detached signature by a namespace key. |

A Profile **MUST NOT** contain secrets, private keys, access tokens, member
lists, or user-specific data. Clients **MUST** ignore unknown members.

## 5. Verification

`signature` is a detached Ed25519 signature over the RFC 8785 JCS canonical
form of the Profile with the `signature` member removed. Signers **MUST** use
an `active` namespace key.

A client or provider accepts a Profile only when all of the following hold:

1. `schema` is `actr.realm/v1`, and `realm.namespace` and `realm.local_name`
   match the Realm reference being resolved;
2. `signature.key_id` names a key listed in the accepted `realm_namespace`
   member (Actr Authority Discovery, Section 4.4), and the signature verifies
   with that key;
3. `issuer.id` equals `actr:realm:<realm-ref>:authority`;
4. if a Profile for this Realm was accepted before:
   - `sequence` is not lower than the highest accepted `sequence`, and if
     equal, the Profile is identical to the accepted one;
   - `provider.id` equals that of the accepted Profile;
   - the accepted Profile is not retired.

Validity depends only on these checks, never on the URL the Profile came from.
A provider that hosts a Profile is therefore not a trust root.

A Profile with `status` `retired` is a permanent tombstone. After accepting
it, a client **MUST** stop using the Realm.

## 6. Using a Realm

A client reaches a Realm only through the issuer and provider named by its
accepted Profile. Actor credentials for the Realm are verified with
`issuer.keys`, and their issuer **MUST** equal `issuer.id`. Membership of a
Realm admits a principal to the instance's network; roles and permissions
inside the instance are decided by the application, not by this document.

A client **SHOULD** refresh the namespace's `realm_namespace` member and the
Profile from its `realms` location when it establishes a session with the
Realm, and at least every 24 hours while it stays connected. A provider
**SHOULD** refresh both for the Realms it serves at the same interval, and
stops issuing credentials for a Realm whose latest Profile is retired or no
longer lists the issuer key it signs with.

If the key that signed an accepted Profile has been removed from the
namespace's `keys` (a revocation; Actr Authority Discovery, Section 4.4), the
client or provider discards that Profile together with its `sequence` and
`provider.id` baseline, and falls back to the last accepted Profile signed by
a still-listed key; if there is none, it obtains the Profile again as if none
had been accepted. This lets a namespace owner purge Profiles signed with a compromised
key, including ones with an inflated `sequence`, by removing the key and
re-signing the affected Profiles with an active key.

When a refresh finds the Profile unavailable, the client keeps using its
accepted Profile. When a fetched Profile fails verification, the client
ignores it, keeps the accepted Profile, and reports the failure. When no
Profile has ever been accepted and none can be obtained, the client **MUST**
report an error. In every case the client **MUST NOT** fall back to the
default services of the authority descriptor or to any other provider.

## 7. Lifecycle

### 7.1 Creation

1. The namespace key holder sends a `create_realm` request (Section 8) to the
   chosen provider. The provider creates a pending Realm, prepares its issuer
   key and endpoints, and returns the unsigned first Profile.
2. The namespace key holder signs that Profile and always submits it with an
   `activate_realm` request. The provider verifies it (Section 5), activates
   the Realm, and returns the Realm's admin token once, in the `activate_realm`
   response. If the provider hosts the `realms` location, it serves the
   Profile from then on; otherwise the namespace owner publishes it there.

In a provider-owned namespace the provider signs the Profile itself, so no
`activate_realm` request is needed (Section 7.3). A pending Realm that is not
activated in time expires and is removed.

### 7.2 Updates

A higher-`sequence` Profile may change the issuer keys, the provider's
endpoints, `display_name`, and `policy`. It **MUST NOT** change `provider.id`.

### 7.3 Provider-owned namespaces

In a provider-owned namespace, the provider holds the namespace keys and
creates Realms for users who have an account with it. The Host authenticates
the user with the OAuth 2.0 authorization code flow with PKCE (RFC 6749,
RFC 7636). The `href` of the `identity` service link in that namespace's
authority descriptor is the OAuth issuer identifier; the authorization server
metadata is at its RFC 8414 well-known location. The authorization server
**MUST** accept the fixed public client ID `actr-host` without a client
secret, with redirects per RFC 8252 (loopback or private-use URI scheme); the
Host requests scope `realm_management`. The Host then sends `create_realm` and
`reissue_admin_token` with the access token as a bearer token (RFC 6750)
instead of an envelope signature; the provider signs the Profile itself and
returns the admin token in the `create_realm` response. A Host that lost that
response recovers the token with `reissue_admin_token`.

Quotas and the Realm's management rights, including reissuing its admin
token, are bound to the account, so a user who changes or loses a device only
signs in again. Account registration, sign-in methods, and abuse controls are
up to the provider.

Clients **MUST** show that the provider operator is the trust root of Realms
in a provider-owned namespace.

In a namespace owned by an organization, the organization's signing service
authenticates members under its own policy and signs requests and Profiles on
their behalf. Version 1 defines no delegated creation keys.

### 7.4 Retirement

Retirement has one path: the namespace owner publishes a Profile with
`status` `retired`. Providers learn it by refreshing, write a local tombstone,
and stop serving the Realm. The local name is never reused in that namespace.
A retired Profile keeps the same `provider.id` and has a higher `sequence`.
If the Realm's provider has shut down, the namespace owner points the `realms`
link at a location it controls and publishes the retired Profile there. A
provider-owned namespace disappears with its provider: its Realms cannot be
retired and can only be recreated elsewhere.

### 7.5 Namespace key loss

If every namespace private key is lost, existing Realms can no longer receive
new Profiles. The domain owner may publish a new namespace key, which clients
treat as an identity change (Actr Authority Discovery, Section 4.4), and create
new Realms under new local names. Keeping the lost keys listed as `retired`
lets existing Realms keep working with their current Profiles.

## 8. Provider management requests

A provider accepts these management requests as HTTPS `POST` requests to the
`realm-management` service link of its own authority descriptor,
`https://<provider_id>/.well-known/actr` (Actr Authority Discovery,
Section 4.3):

| `op` | Purpose |
| --- | --- |
| `create_realm` | Create a pending Realm and return its unsigned first Profile. |
| `activate_realm` | Submit the signed first Profile in `profile`; the response carries the admin token once. |
| `reissue_admin_token` | Issue a new admin token and invalidate the old one. |

In a DNS namespace, each request is signed by an active namespace key using
the envelope below. In a provider-owned namespace, requests carry the same
payload, including `provider_id`, `nonce`, and validity, without a signature,
and are authorized by the account's bearer access token (Section 7.3).

The payload of a `create_realm` request is:

```json
{
  "op": "create_realm",
  "namespace": "dns:acme.com",
  "local_name": "k7m2q9xh",
  "provider_id": "provider.example",
  "requester": "<opaque member id>",
  "nonce": "<random>",
  "issued_at": "2026-07-29T00:00:00Z",
  "expires_at": "2026-07-29T00:05:00Z"
}
```

Other operations use the same envelope with their own `op` and fields.
`requester` is optional: an organization's signing service sets it to an
opaque identifier of the member it signs for. Other specifications **MAY**
reuse this envelope with their own `op` values; for example, Actr Authority
Discovery, Section 4.5, uses it to register a publisher.

- The payload is canonicalized with RFC 8785 JCS and accompanied by a detached
  Ed25519 signature and its `key_id`.
- `provider_id` **MUST** equal the receiving provider's canonical DNS
  authority, so a request cannot be replayed at another provider.
- `expires_at` minus `issued_at` **MUST NOT** exceed five minutes. Providers
  reject requests outside that window.
- Providers record each `nonce` until the request expires and reject its
  reuse. The nonce prevents only byte-level replay.
- `create_realm` is idempotent on `(namespace, local_name, requester)`, where
  the requester is the `requester` field (absent means empty) or, in a
  provider-owned namespace, the account. A repeated request from the same
  requester for its own pending or active Realm returns the Realm's state,
  without an admin token. A request that names a Realm of another requester,
  or a retired Realm, fails with a conflict.
- `activate_realm` on an already active Realm fails with a conflict. A lost
  admin token is recovered only with `reissue_admin_token`.

Before accepting a signed request, a provider verifies the signing namespace
key through Actr Authority Discovery under the same rules as clients. Because
this fetch happens before the signature is verified, a provider **MUST**
rate-limit it per source address and per target domain and **SHOULD** cache the results,
including failures. A provider **SHOULD** enforce quotas per namespace and per
Realm.

## 9. Security and privacy considerations

- Namespace keys are high-value keys. Offline or hardware storage is strongly
  recommended; the provider does not hold namespace keys for third-party
  domains.
- Profiles do not expire. A client that has never seen a newer Profile may
  accept an older one delivered out of band, for example in an old
  invitation. Refreshing from the `realms` location bounds this window, and
  `sequence` prevents rollback for any client that has seen the newer
  Profile.
- Forbidding fallback to default services prevents an attacker who blocks
  Profile retrieval from redirecting clients to an unauthorized provider.
- The authority descriptor does not list Realms, but a Realm reference reveals
  its namespace and local name wherever it is shared. Hosts **SHOULD**
  generate a random local name by default and keep the readable title in
  `display_name`.
- A compromised namespace key is revoked by removing it from `keys` and
  re-signing the affected Profiles (Section 6). A client that has not yet
  seen the updated `realm_namespace` member cannot tell; this is a residual
  risk bounded by refreshing.
- Removing a key also drops the retired tombstones and `provider.id`
  baselines that rest on Profiles it signed. This is an accepted consequence
  inside the namespace owner's trust boundary: "a local name is never reused"
  is an obligation on the owner, which clients can enforce only for Profiles
  signed by still-listed keys. Provider-local tombstones are unaffected.
- Hosts use a distinct device key in each Realm, so Realms cannot correlate a
  device by its key. In a provider-owned namespace, the provider can link the
  Realms created by one account, which it already signs for; Realm members do
  not see the account.

## 10. Open work

- Define machine-readable JSON Schemas for `actr.realm/v1` and management
  requests, with signing test vectors.
- Define delegated creation keys with scoped local-name prefixes and
  lifetimes.

## 11. References

- [Actr URI](uri.md)
- [Actr Authority Discovery](authority-discovery.md)
- [RFC 3339: Date and Time on the Internet: Timestamps](https://www.rfc-editor.org/rfc/rfc3339.html)
- [RFC 4648: The Base16, Base32, and Base64 Data Encodings](https://www.rfc-editor.org/rfc/rfc4648.html)
- [RFC 6749: The OAuth 2.0 Authorization Framework](https://www.rfc-editor.org/rfc/rfc6749.html)
- [RFC 6750: The OAuth 2.0 Authorization Framework: Bearer Token Usage](https://www.rfc-editor.org/rfc/rfc6750.html)
- [RFC 7636: Proof Key for Code Exchange by OAuth Public Clients](https://www.rfc-editor.org/rfc/rfc7636.html)
- [RFC 8032: Edwards-Curve Digital Signature Algorithm (EdDSA)](https://www.rfc-editor.org/rfc/rfc8032.html)
- [RFC 8252: OAuth 2.0 for Native Apps](https://www.rfc-editor.org/rfc/rfc8252.html)
- [RFC 8414: OAuth 2.0 Authorization Server Metadata](https://www.rfc-editor.org/rfc/rfc8414.html)
- [RFC 8785: JSON Canonicalization Scheme (JCS)](https://www.rfc-editor.org/rfc/rfc8785.html)
