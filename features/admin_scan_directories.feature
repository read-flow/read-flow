@admin_scan_directories
Feature: Scan directory configuration
  An owner can configure which directories the scanner watches. REST and
  COSMIC manage the booted backend's own config directly — "viewing" needs no
  navigation. The PWA's admin UI manages a *remote* instance's configuration
  through a registered source, the same precondition `remotes_manage`/
  `admin_server_settings` document.

  @rest @cosmic @pwa
  Scenario: Adding a scan directory makes it appear in the list
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And I am viewing its scan directory configuration
    When I add "/tmp/library" as a scan directory
    Then "/tmp/library" appears in the list of scan directories
