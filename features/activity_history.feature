@activity_history
Feature: Activity history
  ReadFlow records every scan and library mutation as a structured, unlocalized
  audit trail: an operation with ordered child events carrying typed parameters
  and target snapshots. The COSMIC and PWA surfaces render this history with
  localization applied at presentation time only.

  @rest @cosmic
  Scenario: Scan observations are recorded as activity
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And a document is available in a configured scan directory
    When I trigger a library scan
    Then the most recent activity is a scan operation

  @rest @cosmic
  Scenario: Merged documents keep their history
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And two documents have been added to the library
    When I merge the two documents
    Then the merged-away document still has activity
    And that activity includes a document-merge event

  @rest @cosmic
  Scenario: Deleted documents keep their history
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And a document has been added to the library
    When I delete the document
    Then the most recent activity is a file-delete operation
    And the delete activity records a file-deleted event

  @rest @cosmic
  Scenario: Scan dry runs are recorded as proposals
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And scan dry-run is enabled
    And a document is available in a configured scan directory
    When I trigger a library scan
    Then the most recent activity is a scan operation
    And that scan is marked as a dry run
    And the scan activity events are proposals