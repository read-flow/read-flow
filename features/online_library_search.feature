@online_library_search
Feature: Online library search
  The server aggregates search results from configured OPDS catalogs. With no
  catalogs configured the endpoint still responds successfully with an empty
  result set, confirming the search pipeline is reachable.

  @rest @cosmic @pwa
  Scenario: Searching the online library returns a response
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    When I search the online library for "test"
    Then the online library search responds successfully
