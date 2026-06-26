@sources_delete
Feature: Delete a document from the library
  # DELETE /files/<guid> removes a file record and its associated content rows.
  # REST and Cosmic drive the DAO/endpoint directly; the PWA drives the
  # "Manage" → "Delete this format" → "Delete" confirmation on the document
  # detail page.

  @rest @cosmic @pwa
  Scenario: Deleting a file removes it from the file index
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And a document has been added to the library
    When I delete the document
    Then the file no longer appears in the file index
