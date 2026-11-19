# Loran Admin

App package verification is optional. The standard Loran installation does
not include actr. To enable **Verify package archives**, install an actr CLI
that supports `actr realm check-selection` on the AFP server. Put the binary
on the Loran service's PATH or set `LORAN_ADMIN_ACTR_BIN` to its absolute
path, then restart the service. Check the service's environment, which can
differ from your interactive shell.

The Admin UI disables the preview check when the CLI is unavailable.

## Local Front

On **Create app**, select **Local .actr file** as the Front source. Enter the
file name or absolute path as it appears on the Actrium client and the
Publisher signing key ID. A fixed archive needs its SHA-256 digest. For builds
signed again with the same key, select **Allow rebuilds signed by this key**.
The signed Profile carries the selection, including the path. The client
requests permission and checks the archive before opening it.

## Package releases

After the Owner connects an Admin device, use **Release packages** from the
App detail page. Set `LORAN_PACKAGE_VERIFIER` in the AFP service environment
to the current `actr` executable. The Issuer verifies the proposed archives
against its Publisher pins before signing an updated Profile.

To enable Service cutovers, also set `LORAN_ADMIN_READINESS_DIR` to a directory
managed by the deployment operator. Create one subdirectory per full App ID
such as `echo@realm.example` with these files.

| File | Contents |
| --- | --- |
| `actr.toml` | Probe runtime configuration for this Realm's Issuer and signaling, with a dedicated Realm member key and trust for the Front Publisher. |
| `probe.json` | For example, `{"route":"example.Health.Ready"}`. Use a read-only RPC that succeeds when the target can receive traffic. |
| `request.bin` | Optional encoded RPC request. The verifier sends an empty body when this file is absent. |

Use a downloadable Front archive for Service cutovers so AFP can verify its
contract and use it for the readiness call. Unconfigured probes, failed
verification and unsuccessful RPCs leave the current Profile in place.
A rollback of a Service release checks the previous Service again.
