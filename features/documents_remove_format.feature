@documents_remove_format
Feature: Remove a format from a merged document
  All copies of one format (a single fingerprint) can be removed from a merged
  document in one operation. The files are removed from disk and their records
  deleted; the document itself is deleted when it was the last format remaining.

  @rest @cosmic @pwa
  Scenario: Removing one format keeps the other format
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And two documents have been added to the library
    When I merge the two documents
    And I remove the second document's format
    Then the merged document has only its original format
    And the removed format's files are gone from the file index

  Scenario: Removing the only format deletes the document
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And a document has been added to the library
    When I remove the current document's format
    Then the document no longer exists in the library