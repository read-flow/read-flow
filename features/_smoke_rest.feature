@rest
Feature: BDD harness smoke test (Rust/REST)
  Canary scenario for the cucumber-rs harness — proves a real backend boots
  and the RestDriver can reach it over HTTP. Kept separate from
  `_smoke.feature` (the PWA canary): "open the app" / "see a heading" has no
  REST analogue, so each runner's canary exercises what's natural for that
  surface. Not a parity scenario — keep this fast and stable.

  Scenario: The status endpoint reports healthy
    Given a read-flow server is running
    When I check its status
    Then the status is healthy
