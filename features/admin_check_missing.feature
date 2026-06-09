@admin_check_missing
Feature: Check missing files
  # POST /maintenance/check-missing scans the database for file records whose
  # path no longer exists on disk and returns a list of such paths. REST and
  # Cosmic drive the endpoint / application-module method directly; the PWA
  # drives the "Check missing files" button on the admin page.

  @rest @cosmic @pwa
  Scenario: Checking an empty library reports no missing files
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    When I run the check-missing operation
    Then no files are reported as missing
