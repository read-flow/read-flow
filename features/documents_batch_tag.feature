@documents_batch_tag
Feature: Batch-tag documents
  Users select one or more documents in the list view and add or remove a
  tag to all of them at once.

  @cosmic @pwa
  Scenario: Adding a tag to a document selection
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And a document has been added to the library
    When I batch-add tag "fiction" to the selected documents
    Then "fiction" appears in the document's tag list
