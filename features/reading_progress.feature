@reading_progress
Feature: Reading progress
  # PUT /reading-state persists the reader's current position (a serialised
  # CFI/page string) and completion percentage for a given file fingerprint.
  # REST and Cosmic drive the endpoint / DAO directly; the PWA scenario is
  # deferred since reading progress is set only by the embedded viewer
  # (PDF.js / epub.js) and there is no progress readout in the library UI.

  @rest @cosmic
  Scenario: Setting reading progress persists the position and percentage
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And a document has been added to the library
    When I set the reading progress to 50% at position "chapter-2"
    Then the reading progress is 50% at "chapter-2"
