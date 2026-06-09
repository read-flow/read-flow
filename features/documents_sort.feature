@documents_sort
Feature: Sort documents
  Users sort the document list by title, filename, size, type, or status.
  The sort direction (ascending / descending) can be toggled.

  @cosmic @pwa
  Scenario: Sorting by title ascending places earlier titles first
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And two documents have been added to the library
    When I sort the documents by title ascending
    Then "BDD Sample Book" appears before "Zeta Test Book" in the list
