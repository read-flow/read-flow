@documents_list
Feature: Document list
  The library aggregates scanned files into documents and surfaces them for
  browsing — reuses the shared `features/fixtures/sample.epub` fixture (titled
  "BDD Sample Book") via each driver's `seed_document` (see `tags_list.feature`'s
  doc comment for why a real, parseable EPUB is required).

  @rest @cosmic @pwa
  Scenario: A scanned document appears in the library's list of documents
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And a document has been added to the library
    Then "BDD Sample Book" appears in the library's list of documents
