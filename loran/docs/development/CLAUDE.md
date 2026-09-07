# Historical Development Notes

This file previously contained conclusions about legacy KS/MFR, multi-Realm, and "internal use security" that are no longer development guidelines.

Current development entry points:

- [`../../README.md`](../../README.md)
- [`../REALM_V1_ARCHITECTURE.zh.md`](../REALM_V1_ARCHITECTURE.zh.md)
- [`../THREAT_MODEL_REALM_V1.zh.md`](../THREAT_MODEL_REALM_V1.zh.md)
- [`../DEVELOPMENT.md`](../DEVELOPMENT.md)
- [`../CRATES.md`](../CRATES.md)

Any change must maintain the single Realm database constraint, credential scope, default deny,
secrets not in logs, and offline explicit migration. It is not allowed to restore removed paths
using default keys, plaintext passwords, optional authentication, or "intranet only" assumptions
from old documentation.
