@admin_authorized_users
Feature: Authorized users
  An owner can manage which users may authenticate against a read-flow
  instance. REST and COSMIC manage the booted backend's own users directly —
  "viewing" needs no navigation. The PWA's admin UI manages a *remote*
  instance's users through a registered source, the same precondition
  `admin_scan_directories`/`admin_server_settings` document.

  @rest @cosmic @pwa
  Scenario: Adding a user makes them appear in the list of authorized users
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And I am viewing its authorized users
    When I add a user "bob" with passphrase "wonderland-tea-party"
    Then "bob" appears in the list of authorized users
