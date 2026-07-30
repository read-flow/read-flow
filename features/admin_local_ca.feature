@admin_local_ca
Feature: Local certificate authority
  A local root CA lets the server issue browser-trusted TLS certs without a
  public hostname: a client trusts the CA root once (via `/ca.pem`), and every
  leaf cert issued afterwards — even regenerated ones — is automatically
  trusted without a further per-cert step. COSMIC's Preferences "Generate
  Certificate" button drives generation; REST's role is passively serving the
  CA root at `/ca.pem` once TLS is configured.

  @rest @cosmic
  Scenario: Generating a CA-signed certificate makes its CA root available at /ca.pem
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    When I generate a local CA-signed certificate
    Then the CA root certificate is served at /ca.pem
