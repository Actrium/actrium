Public test-only TLS material. Never use these private keys in a deployment.

ca.pem signs cert.pem, which covers localhost and 127.0.0.1 and exercises the
TLS transport lanes. endpoint-mismatch-ca.pem signs endpoint-mismatch-cert.pem,
which covers only the localhost DNS name (no IP SAN) and exercises
endpoint-name rejection for a dialed IP on any platform. Generated with
OpenSSL for ten-year validity in October 2026; renew before October 2036. The
CA private keys are not retained.
