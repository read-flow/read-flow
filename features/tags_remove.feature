@tags_remove
Feature: Remove a tag from a document
  A tag previously applied to a document can be removed. After removal
  the tag no longer appears in that document's tag list.

  @rest @cosmic @pwa
  Scenario: Removing a tag removes it from the document's tag list
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And a document tagged "fiction" has been added to the library
    When I remove the tag "fiction" from the document
    Then "fiction" no longer appears in the document's tag list
