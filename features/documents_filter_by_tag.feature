@documents_filter_by_tag
Feature: Filter documents by tag
  Users narrow the document list to documents that carry a specific tag.

  @cosmic @pwa
  Scenario: Filtering by tag shows matching documents
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And a document tagged "sci-fi" has been added to the library
    When I filter by tag "sci-fi"
    Then "BDD Sample Book" appears in the filtered results
