# App development guidance

> Non-normative document. This is advice for writing Actrium apps, not a platform rule; the
> platform does not check it and does not enforce it.

## 1. What the platform guarantees, and what it does not

- The platform guarantees: the peer's Actor credential proves it holds a certain key, owns a
  certain ActorId, belongs to a certain Realm, and declares a certain Actor type.
- The platform does not guarantee: what code the peer is actually running. The local Actr Host
  verifies the package it runs itself, but a remote device is controlled by someone else, and a
  modified client can obtain the same credential.
- When needed, a managed Realm can require the peer to present a platform remote attestation
  (platform app attestation, etc.) — see [Runtime and protocol](06-runtime-and-protocol.md) §2.2.

## 2. Recommendations

1. **Authorize in the Service.** Treat Front as an ordinary client: who can read or write, whether
   a business command may run, and whether the Workspace can be accessed, are all decided by the
   Service based on the principal and application permissions, not on whether the UI happens to
   hide a button.
2. **Do not trust fields the Guest self-reports.** User, Workspace, or permission fields in the
   Guest payload are only input; treat the session metadata attached by the Actr Host as
   authoritative.
3. **Do not grant authority based on self-reported information.** The Actor type, Actr Host
   version, or package digest that Front self-reports is used only for compatibility negotiation,
   not as grounds for granting a high-risk permission; when that is genuinely needed, require
   remote attestation.

## 3. When it is safe to relax this

When Front and Service run on the same device, under the same Actr Host (e.g., a local Realm, or
single-user use), both pieces of code have already been verified by that device. In that case a
"modified client" can only be the user themselves, and the benefit of the recommendations above is
small.
