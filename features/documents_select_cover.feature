@documents_select_cover
Feature: Document cover selection
  Users can explicitly set which cover image is used for a document. Setting
  the cover fingerprint via the metadata update endpoint persists the choice
  and the cover endpoint serves the selected image.

  @rest @cosmic @pwa
  Scenario: Setting a cover fingerprint makes the selected cover available
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And a document with a cover image has been added to the library
    When I set the document's cover to its file's cover image
    Then a cover image is returned
