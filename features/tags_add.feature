@tags_add
Feature: Add a tag to a document
  Tags help organise the library. Adding a tag to a document makes it
  available for filtering and browsing, and persists across restarts.

  @rest @cosmic @pwa
  Scenario: Adding a tag makes it appear in the document's tag list
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And a document has been added to the library
    When I add the tag "fiction" to the document
    Then "fiction" appears in the document's tag list
