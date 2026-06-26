@documents_search
Feature: Document search
  Users narrow the document list by typing a keyword into the search box.
  The search targets title, authors, tags, and file path.

  @cosmic @pwa
  Scenario: Searching by keyword shows matching documents
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And a document has been added to the library
    When I search for "BDD"
    Then "BDD Sample Book" appears in the search results
