@reading_status
Feature: Reading status
  Users track their reading progress by marking a document as Unread,
  Reading, or Read. The status persists on the server and is reflected
  back to all clients.

  @rest @cosmic @pwa
  Scenario: Setting reading status persists
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And a document has been added to the library
    When I set the document's reading status to "Reading"
    Then the document's reading status is "Reading"
