@admin_scan
Feature: Library scan
  # The PWA triggers a scan of a registered source via the admin page's
  # "Scan library" button. REST/Cosmic trigger POST /scan directly, having
  # pre-configured the scan directory in the Given step. All three drivers
  # use the shared `features/fixtures/sample.epub` as the scannable file.

  @rest @cosmic @pwa
  Scenario: Scanning a configured directory processes documents
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And a document is available in a configured scan directory
    When I trigger a library scan
    Then the scan reports at least 1 document processed
