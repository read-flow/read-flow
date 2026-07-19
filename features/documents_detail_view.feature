@documents_detail_view
Feature: Document detail view
  # Viewing a document's detail page shows its metadata, including the title
  # extracted during scan. The fixture EPUB's dc:title is "BDD Sample Book"
  # (see `features/fixtures/sample.epub`), which every driver seeds via its
  # `seed_document` helper.

  @rest @cosmic @pwa
  Scenario: Document detail shows the book's title
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And a document has been added to the library
    When I view the document's details
    Then the document's title is "BDD Sample Book"

  @rest @cosmic
  Scenario: A scanned document's source records when it was imported
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And a document has been added to the library
    When I view the document's details
    Then the document's source shows an import timestamp
