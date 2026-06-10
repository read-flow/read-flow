@documents_merge
Feature: Merge documents
  Duplicate documents (e.g. the same book uploaded twice from different sources)
  can be merged into a single document. The winner document keeps its identity;
  the loser's files are re-assigned to it.

  @rest @cosmic @pwa
  Scenario: Merging two documents leaves one document
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And two documents have been added to the library
    When I merge the two documents
    Then only one document remains in the library
