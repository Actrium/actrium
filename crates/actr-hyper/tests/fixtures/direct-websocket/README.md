Public test-only TLS material. Never use this private key in a deployment.
The leaf is signed by ca.pem and covers localhost and 127.0.0.1, deliberately
excluding 127.0.0.2 for endpoint-name rejection coverage. Generated with OpenSSL
for a ten-year validity period in September 2026; renew before September 2036.
The CA private key is not retained.
