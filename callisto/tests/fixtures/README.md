# Test fixtures

`service_auth_test_key.pem` is a throwaway RSA key generated for the
service-login unit tests, which sign their own Google-shaped identity tokens
with it. It protects nothing and is trusted by nothing outside those tests.
`service_auth_test_key.n` is its modulus, base64url-encoded, as Google
publishes keys.
