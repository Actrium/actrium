# Loran Admin

App package verification is optional. The standard Loran installation does
not include actr. To enable **Verify package archives**, install an actr CLI
that supports `actr realm check-selection` on the AFP server. Put the binary
on the Loran service's PATH or set `LORAN_ADMIN_ACTR_BIN` to its absolute
path, then restart the service. Check the service's environment, which can
differ from your interactive shell.

The Admin UI disables package verification when the CLI is unavailable.
Creating an App and saving its package selection remain available.
