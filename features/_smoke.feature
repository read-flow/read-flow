Feature: BDD harness smoke test
  Canary scenario that proves the BDD plumbing works end to end:
  a server boots, the app loads against it, and the runner can assert
  on the rendered page. Not a parity scenario — keep this fast and stable.

  Scenario: The library page loads
    Given a read-flow server is running
    When I open the app
    Then I see the library heading
