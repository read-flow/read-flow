@documents_filter_by_source
Feature: Filter documents by source
  The document list can be narrowed to show only documents available from a
  specific source. This is a client-side filter: REST has no equivalent
  endpoint, so only the COSMIC and PWA surfaces exercise it.

  @cosmic @pwa
  Scenario: Filtering by Local source shows documents from the local library
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And a document has been added to the library
    When I filter documents by source "Local"
    Then "BDD Sample Book" appears in the filtered document list
