@documents_filter_by_status
Feature: Filter documents by reading status
  Users narrow the document list to a specific reading status (Unread /
  Reading / Read).

  @cosmic @pwa
  Scenario: Filtering by reading status shows matching documents
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And a document has been added to the library
    And I set the document's reading status to "Reading"
    When I filter by reading status "Reading"
    Then "BDD Sample Book" appears in the filtered results
