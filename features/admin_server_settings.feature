@admin_server_settings
Feature: Server settings
  An owner can view and edit a read-flow instance's runtime settings (scan
  extensions, concurrency, dry-run mode, private mode, private tags). REST and
  COSMIC manage the booted backend's own config directly — "viewing" needs no
  navigation. The PWA's admin UI manages a *remote* instance's settings through
  a registered source, the same precondition `remotes_manage` documents.

  @rest @cosmic @pwa
  Scenario: Enabling dry-run mode persists
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And I am viewing its server settings
    When I enable dry-run mode and save
    Then dry-run mode is reported as enabled
