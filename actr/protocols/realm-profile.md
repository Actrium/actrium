# Actr Realm Profile

Language: **English** | [简体中文](realm-profile.zh.md)

Status: initial draft.

This document defines how an Actr Host or Realm Provider resolves a Realm
name to a verified Realm Profile, the `actr.realm/v1` Realm Profile format,
and the Realm management requests a Realm Provider accepts.

## 1. Scope

A Realm is the trust boundary of one application instance. Creating an
application instance creates a new Realm. A Realm is named `<realm_id>@<domain>`,
for example `k7m2q9xh@acme.com`; the syntax is defined in
[Actr URI](uri.md), Section 5.1.

A Realm Profile is a signed, public document that says which package the
instance runs and which Issuer and which Realm Provider serve the Realm. A Realm signing key of the Realm domain signs
it, so the Realm Profile can be stored anywhere: on the domain's own static
hosting, at a Realm Provider, or embedded in a Realm invitation. Where it was
fetched from does not affect its validity.

A Realm is served by one Realm Provider for its whole life. A Realm Provider
is a running deployment, run by some operator, that provides the Realm's
signaling, TURN, and Issuer. Moving an application to another operator means
creating a new Realm there and migrating application data; the old Realm is
retired.

This document does not define the signaling, identity, or ICE protocols a
Realm Provider exposes, the Actor credential format, or Realm membership and
application permissions inside an application instance.

The keywords **MUST**, **MUST NOT**, **SHOULD**, **SHOULD NOT**, and **MAY** are
to be interpreted as described by BCP 14 when they appear in uppercase.

## 2. Realm domains

Every Realm lives under a Realm domain, which serves only as its naming root:
an existing, globally unique name whose ownership can be verified. Without a
domain there is no Realm, except for local Realms (Section 2.2).

### 2.1 DNS domains

A DNS Realm domain is the domain in `actr://<domain>`. Its owner is the **Realm signing key holder**: its Ed25519
Realm signing keys are the long-term trust root of every Realm under the
domain. Their publication, rotation, and rollback protection are defined by
[Actr Authority Discovery](authority-discovery.md), Section 4.4. A domain
publishes its Realm signing keys once; creating a Realm does not change the
domain's hosting.

The domain owner may hold the keys itself, typically in a signing service, or
delegate their custody to a Realm Provider. A person without a domain of their
own obtains Realms under a Realm Provider's domain; that Realm Provider owns
the domain and holds its keys. Whoever holds the keys is the trust root of the
domain's Realms.

A Realm is **under a Realm Provider's domain** when, compared as normalized
A-labels, its Realm domain equals the Realm Provider ID of its Realm Profile or
ends with `"."` followed by that Realm Provider ID (on a label boundary). Only
in that case do Actr Hosts show the Realm Provider as the trust root; otherwise
they show the Realm domain. A domain owner that delegated custody of its keys
to a Realm Provider is not visible to Actr Hosts (Section 9).

### 2.2 Local Realms

A local Realm uses the pseudo-domain `<installation-id>.actr.alt` under the
RFC 9476 `.alt` suffix. `installation-id` is 128 random bits generated at
installation, encoded as 26 characters of lowercase RFC 4648 base32 without
padding.

Actr Hosts **MUST NOT** resolve `.actr.alt` names through DNS or send them to
any Realm Provider. Local Realms have no Realm Profile, no Realm Provider, and
no Realm management requests; the Actr Host keeps a local record for each one, holding at least the package
the instance runs (download address or local file, plus SHA-256).
A local Realm name is usable only on that installation. The rest of this
document applies to DNS Realm domains only.

## 3. Locating a Realm Profile

For a Realm name `<realm_id>@<domain>`, an Actr Host:

1. obtains and verifies the authority descriptor of `actr://<domain>` and its
   `realm_signing_keys` member, as defined by Actr Authority Discovery;
2. selects the service links with `rel` `realms` and protocol
   `actr-realm-profile/1`;
3. for each link in document order, removes any trailing `/` from `href` and
   sends `GET <href>/<realm_id>` with `Accept: application/json`, stopping at the
   first response that yields a valid Profile.

A `realms` href **MUST NOT** contain a query or fragment. It **MAY** point to
the domain's own hosting or to a Realm Provider, and **MAY** carry an explicit
port. If no link yields a valid Realm Profile (because of `404 Not Found`,
another error, a timeout, or an invalid document), the Realm Profile is
unavailable.

The request is public. Actr Hosts **MUST** use HTTPS and **MUST NOT** send
`Authorization` headers, cookies, or other credentials. Redirects follow the
rules of Actr Authority Discovery: at most five, every hop over HTTPS. A
privileged process, such as a Realm Provider, **MUST** additionally apply
server-side request forgery checks on every hop: reject non-public addresses,
pin each hop to the pre-checked address, and limit the response to 64 KiB.

A Realm Profile **MAY** also be delivered out of band, for example embedded in
a Realm invitation. An Actr Host verifies such a Realm Profile exactly as one
fetched over HTTPS.

## 4. Profile format

A Realm Profile is a UTF-8 JSON object. Member names use snake_case.

```json
{
  "schema": "actr.realm/v1",
  "sequence": 31,
  "realm": {
    "name": "k7m2q9xh@acme.com",
    "display_name": "Acme Notes",
    "status": "active"
  },
  "package": {
    "href": "https://cdn.notes.example/team-notes-1.4.2.actr",
    "sha256": "<64 hex>"
  },
  "issuer": {
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
  "signature": { "key_id": "rs-2026-1", "algorithm": "Ed25519", "value": "<base64>" }
}
```

| Member | Meaning |
| --- | --- |
| `schema` | **MUST** be `actr.realm/v1`. |
| `sequence` | Realm Profile sequence: a non-negative integer that increases with every Realm Profile published for this Realm. |
| `realm.name` | The Realm name. It **MUST** equal the name being resolved. |
| `realm.display_name` | Human-readable title. It is not an identifier. |
| `realm.status` | `active` or `retired`. |
| `package.href` | HTTPS download address of the package the instance runs; it may be anywhere. |
| `package.sha256` | SHA-256 of all bytes of that package, in lowercase hexadecimal. |
| `issuer.url` | Registration and verification endpoint of the Issuer. |
| `issuer.keys` | Realm issuer public keys used to verify Actor credentials. |
| `provider.id` | Realm Provider ID: the Realm Provider's canonical DNS authority. It never changes during the Realm's life. |
| `provider.signaling` | Signaling endpoints. Entries sharing a `control_plane_id` are replicas or failover entries of one Realm Provider control plane, not a federation. |
| `provider.ice` | STUN and TURN endpoints. TURN entries name the `credential_audience` for short-lived TURN credentials. |
| `policy` | Join policy of the Realm. |
| `issued_at` | RFC 3339 time the Realm Profile was signed. It is informational; Realm Profiles do not expire. |
| `signature` | Detached signature by a Realm signing key. |

When `realm.status` is `active`, the Realm Profile **MUST** include `package`.

A Realm Profile **MUST NOT** contain secrets, private keys, access tokens,
member lists, or user-specific data. Actr Hosts **MUST** ignore unknown
members.

## 5. Verification

`signature` is a detached Ed25519 signature over the RFC 8785 JCS canonical
form of the Realm Profile with the `signature` member removed. The Realm
signing key holder **MUST** use an `active` Realm signing key.

An Actr Host or Realm Provider accepts a Realm Profile only when all of the
following hold:

1. `schema` is `actr.realm/v1`, and `realm.name` equals the Realm name being
   resolved;
2. `signature.key_id` names a key listed in the accepted `realm_signing_keys`
   member of the Realm's domain (Actr Authority Discovery, Section 4.4), and
   the signature verifies with that key;
3. if a Realm Profile for this Realm was accepted before:
   - its Realm Profile sequence is not lower than the highest accepted one,
     and if equal, the Realm Profile is identical to the accepted one;
   - `provider.id` equals that of the accepted Realm Profile;
   - the accepted Realm Profile is not retired.

Validity depends only on these checks, never on the URL the Realm Profile came
from. A Realm Provider that stores a Realm Profile is therefore not a trust
root.

A retired Realm Profile (`status` `retired`) is permanent. After accepting
it, an Actr Host **MUST** stop using the Realm.

## 6. Using a Realm

An Actr Host downloads `package.href` with an HTTPS GET that carries no
credentials, **MUST** check `package.sha256`, and verifies the package
signature as in [Actor Package](actor-package.zh.md) (Chinese only); if any
check fails, it does not run the package. The Actor type declared in the
package manifest is the instance's program: the instance's Service is the one
Actor in this Realm with that Actor type; finding none or more than one is an
error.

An Actr Host reaches a Realm only through the Issuer and Realm Provider named
by its accepted Realm Profile. Actor credentials for the Realm name the Realm
as their issuer and are verified with the Realm issuer keys in `issuer.keys`.
Realm membership means that a client has registered its Realm client key in
the Realm and has not been revoked; it admits the client to the instance's
network, and the Issuer issues Actor credentials only to registered,
unrevoked Realm client keys; application
permissions inside the instance are decided by the application, not by this
document.

An Actr Host **SHOULD** refresh the domain's `realm_signing_keys` member and
the Realm Profile from its `realms` location when it establishes a session
with the Realm, and at least every 24 hours while it stays connected. A Realm
Provider **SHOULD** refresh both for the Realms it serves at the same
interval, and stops issuing Actor credentials for a Realm whose latest Realm
Profile is retired or no longer lists the Realm issuer key it signs with.

If the key that signed an accepted Realm Profile has been removed from the
domain's `keys` (a revocation; Actr Authority Discovery, Section 4.4), the
Actr Host or Realm Provider discards that Realm Profile together with its
Realm Profile sequence and `provider.id` baseline, and falls back to the last
accepted Realm Profile signed by a still-listed key; if there is none, it
obtains the Realm Profile again as if none had been accepted. This lets a
domain owner purge Realm Profiles signed with a compromised key, including
ones with an inflated sequence, by removing the key and re-signing the
affected Realm Profiles with an active key.

When a refresh finds the Realm Profile unavailable, the Actr Host keeps using
its accepted Realm Profile. When a fetched Realm Profile fails verification,
the Actr Host ignores it, keeps the accepted one, and reports the failure.
When no Realm Profile has ever been accepted and none can be obtained, the
Actr Host **MUST** report an error. In every case the Actr Host **MUST NOT**
use any Realm Provider other than the one named by the accepted Realm Profile,
including services listed in the authority descriptor.

## 7. Lifecycle

### 7.1 Creation

The Actr Host, the user, or the key holder's policy chooses the Realm
Provider. Every step is authorized by the Realm signing key holder
(Section 7.3).

1. A `create_realm` request (Section 8) goes to the chosen Realm Provider.
   The Realm Provider creates a pending Realm, prepares its Realm issuer key
   and endpoints, and returns the unsigned first Realm Profile.
2. The Realm signing key holder signs that Realm Profile and always submits it
   with an `activate_realm` request. The Realm Provider verifies it
   (Section 5), activates the Realm, and returns the Realm admin token once, in
   the `activate_realm` response. If the Realm Provider serves the `realms`
   location, it serves the Realm Profile from then on; otherwise the domain
   owner publishes it there.

For a Realm under the Realm Provider's domain, the Realm Provider signs the
Realm Profile itself, so no `activate_realm` request is needed (Section 7.3). A
pending Realm that is not activated in time expires and is removed.

### 7.2 Updates

A higher-sequence Realm Profile may change `package` (upgrading the program),
the Realm issuer keys, the Realm Provider's endpoints, `display_name`, and
`policy`. It **MUST NOT** change
`provider.id`.

### 7.3 Authorization by the key holder

Creating and activating a Realm and reissuing its Realm admin token require
the authorization of the Realm signing key holder. Version 1 defines no
delegated creation keys. The Actr Host picks the path with the rule of
Section 2.1:

- **A Realm under the Realm Provider's domain.** The Realm Provider
  authenticates the requester by a Realm Provider account. The Actr Host uses
  the OAuth 2.0 authorization code flow with PKCE (RFC 6749, RFC 7636). The
  `href` of the `identity` service link in the Realm Provider's own authority
  descriptor (`https://<Realm Provider ID>/.well-known/actr`) is the OAuth
  authorization server identifier (issuer identifier, RFC 8414); the authorization server metadata is at its RFC 8414 well-known
  location. The authorization server **MUST** accept the fixed public client
  ID `actr-host` without a client secret, with redirects per RFC 8252
  (loopback or private-use URI scheme); the Actr Host requests scope
  `realm_management`. The Actr Host sends `create_realm` and
  `reissue_admin_token` with the access token as a bearer token (RFC 6750)
  instead of an envelope signature; the Realm Provider signs the Realm Profile
  itself and returns the Realm admin token in the `create_realm` response. An
  Actr Host that lost that response recovers the token with
  `reissue_admin_token`. Quotas and the Realm's admin rights, including
  reissuing its Realm admin token, are bound to the Realm Provider account, so
  a user who changes or loses a device only signs in again. Account
  registration, sign-in methods, and abuse controls are up to the Realm
  Provider.
- **Any other Realm.** Each Realm management request is an Actr management
  envelope signed by an active Realm signing key (Section 8), obtained from
  the signing service of the Realm signing key holder, whether that is the
  domain owner or a Realm Provider it delegated custody to. How that signing
  service authenticates the people it signs for is outside this document.

### 7.4 Retirement

Retirement has one path: the domain owner publishes a retired Realm Profile.
Realm Providers learn it by refreshing, write a Realm Provider tombstone, and
stop serving the Realm. The Realm name is never reused. A retired Realm
Profile keeps the same `provider.id` and has a higher sequence. If the Realm's
Realm Provider has shut down, the domain owner points the `realms` link at a
location it controls and publishes the retired Realm Profile there. A domain
owned by a Realm Provider disappears with it: its Realms cannot be retired
and can only be recreated elsewhere.

### 7.5 Realm signing key loss

If every Realm signing private key of a domain is lost, existing Realms can no
longer receive new Realm Profiles. The domain owner may publish a new Realm
signing key, which Actr Hosts treat as an identity change (Actr Authority
Discovery, Section 4.4), and create new Realms under new RealmIds. Keeping
the lost keys listed as `retired` lets existing Realms keep working with their
current Realm Profiles.

## 8. Realm management requests

A Realm Provider accepts these Realm management requests as HTTPS `POST`
requests to the `realm-management` service link of its own authority
descriptor, `https://<provider_id>/.well-known/actr` (Actr Authority
Discovery, Section 4.3):

| `op` | Purpose |
| --- | --- |
| `create_realm` | Create a pending Realm and return its unsigned first Realm Profile (its `package` taken from the request). |
| `activate_realm` | Submit the signed first Realm Profile in `profile`; the response carries the Realm admin token once. |
| `reissue_admin_token` | Issue a new Realm admin token and invalidate the old one. |

For a Realm under the Realm Provider's domain (Section 2.1), requests carry
the payload below, including `provider_id`, `nonce`, and validity, without a
signature, and are authorized by the Realm Provider account's bearer access
token (Section 7.3). For any other Realm, each request is an Actr management
envelope: the payload below plus a signature by an active Realm signing key of
the Realm domain.

The payload of a `create_realm` request is:

```json
{
  "op": "create_realm",
  "realm": "k7m2q9xh@acme.com",
  "package": { "href": "https://cdn.notes.example/team-notes-1.4.2.actr", "sha256": "<64 hex>" },
  "provider_id": "provider.example",
  "requester": "<opaque member id>",
  "nonce": "<random>",
  "issued_at": "2026-07-29T00:00:00Z",
  "expires_at": "2026-07-29T00:05:00Z"
}
```

Other operations use the same envelope with their own `op` and fields.
`requester` is optional: a key holder's signing service sets it to an opaque
identifier of the person it signs for. In bearer requests the Realm Provider
**MUST** ignore `requester` and uses the Realm Provider account as the
requester. Other specifications **MAY** reuse the
Actr management envelope with their own `op` values; for example, Actr
Authority Discovery, Section 4.5, uses it to register a publisher.

- The payload is canonicalized with RFC 8785 JCS and accompanied by a detached
  Ed25519 signature and its `key_id`.
- `provider_id` **MUST** equal the receiving Realm Provider ID, so a request
  cannot be replayed at another Realm Provider.
- `expires_at` minus `issued_at` **MUST NOT** exceed five minutes. Realm
  Providers reject requests outside that window.
- Realm Providers record each `nonce` until the request expires and reject its
  reuse. The nonce prevents only byte-level replay.
- `create_realm` is idempotent on the Realm Provider idempotency key
  `(realm, requester)`, where the requester is the `requester` field (absent
  means empty) or, for bearer requests, the Realm Provider account. A repeated request from the same requester for its own
  pending or active Realm returns the Realm's state, without a Realm admin
  token. A request that names a Realm of another requester, or a retired
  Realm, fails with a conflict.
- `activate_realm` on an already active Realm fails with a conflict. A lost
  Realm admin token is recovered only with `reissue_admin_token`.

Before accepting a signed request, a Realm Provider verifies the Realm signing
key through Actr Authority Discovery under the same rules as Actr Hosts.
Because this fetch happens before the signature is verified, a Realm Provider
**MUST** rate-limit it per source address and per target domain and **SHOULD**
cache the results, including failures. A Realm Provider **SHOULD** enforce
quotas per Realm domain and per Realm.

## 9. Security and privacy considerations

- Realm signing keys are high-value keys. Offline or hardware storage is
  strongly recommended. A Realm Provider holds Realm signing keys only for
  domains whose owners delegated custody to it, including its own domains.
- Realm Profiles do not expire. An Actr Host that has never seen a newer Realm
  Profile may accept an older one delivered out of band, for example in an old
  Realm invitation. Refreshing from the `realms` location bounds this window,
  and the Realm Profile sequence prevents rollback for any Actr Host that has
  seen the newer Realm Profile.
- Using only the Realm Provider named by the Realm Profile prevents an
  attacker who blocks Realm Profile retrieval from redirecting Actr Hosts to
  an unauthorized Realm Provider.
- The authority descriptor does not list Realms, but a Realm name reveals its
  Realm domain and RealmId wherever it is shared. Actr Hosts **SHOULD**
  generate a random RealmId by default and keep the readable title in
  `display_name`.
- Realm names and ActorIds look like email addresses. Actr Hosts **MUST NOT**
  render them as `mailto:` links and should display them as instance
  addresses ([Actr URI](uri.md), Section 5.1).
- Actr Hosts cannot see that a domain owner delegated custody of its Realm
  signing keys to a Realm Provider; for such Realms they show the Realm domain
  as the trust root. This is an accepted limitation.
- A compromised Realm signing key is revoked by removing it from `keys` and
  re-signing the affected Realm Profiles (Section 6). An Actr Host that has
  not yet seen the updated `realm_signing_keys` member cannot tell; this is a
  residual risk bounded by refreshing.
- Removing a key also drops the retired Realm Profiles and `provider.id`
  baselines that rest on Realm Profiles it signed. This is an accepted
  consequence inside the domain owner's trust boundary: "a Realm name is never
  reused" is an obligation on the owner, which Actr Hosts can enforce only for
  Realm Profiles signed by still-listed keys. Realm Provider tombstones are
  unaffected.
- Actr Hosts use a distinct Realm client key in each Realm, so Realms cannot
  correlate a client by its key. For Realms under its domain, a Realm
  Provider can link the Realms created by one Realm Provider account, which it
  already signs for; Realm members do not see the account. The same holds for a key
  holder's signing service.

## 10. Open work

- Define machine-readable JSON Schemas for `actr.realm/v1` and Realm
  management requests, with signing test vectors.
- Define delegated creation keys with scoped RealmId prefixes and lifetimes.

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
- [RFC 9476: The .alt Special-Use Top-Level Domain](https://www.rfc-editor.org/rfc/rfc9476.html)
