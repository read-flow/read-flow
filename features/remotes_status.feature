@remotes_status
Feature: Remote source status
  A read-flow instance can register another instance as a remote source and
  check whether it's reachable. This is the first feature specified once and
  proven on every surface — see the parity plan for why `remotes.status` was
  picked as the low-friction proof-of-concept (no DB seed data beyond the
  remote itself, config-driven auth, already wired into the PWA UI).

  Each driver maps "add a remote source" onto its own natural shape: the PWA
  drives the real `/settings/sources` "Add source" form; REST has no "add
  remote" concept, so the request becomes "call /status with these creds"
  directly; COSMIC inserts a `Remote` row and drives `CheckSourceStatus`. All
  three report the same observable: is the source reachable?

  @pwa @rest @cosmic
  Scenario: Adding a reachable remote reports it as online
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    When I add that server as a remote source named "Home Server" with user "alice" and passphrase "correct-horse"
    Then the remote source "Home Server" is reported as reachable

  @pwa @rest @cosmic
  Scenario: Adding a remote with wrong credentials reports it as unreachable
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    When I add that server as a remote source named "Home Server" with user "alice" and passphrase "wrong-password"
    Then the remote source "Home Server" is reported as unreachable
