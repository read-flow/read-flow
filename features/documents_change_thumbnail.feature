@documents_change_thumbnail
Feature: Change document thumbnail
  Users can pick a specific PDF page — optionally trimmed of surrounding
  whitespace — to use as a document's thumbnail. The rendered page is stored
  as the cover for that PDF content and becomes the document's cover.

  @rest @cosmic
  Scenario: Setting a PDF page as the thumbnail makes it the document's cover
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And a PDF document has been added to the library
    When I set the document's thumbnail to its first page, untrimmed
    Then a cover image is returned
