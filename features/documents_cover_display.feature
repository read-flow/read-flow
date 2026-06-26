@documents_cover_display
Feature: Document cover display
  Each document may carry an embedded cover image. When present, the cover
  is extracted during scanning and served via a dedicated REST endpoint;
  clients display it as a thumbnail alongside the document title.

  @rest @cosmic @pwa
  Scenario: A document with a cover image has its cover available
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And a document with a cover image has been added to the library
    When I request the document's cover
    Then a cover image is returned
