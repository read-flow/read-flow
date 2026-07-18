@online_library_manage_catalogs
Feature: Manage online library catalogs
  Built-in catalogs can be toggled on/off; custom OPDS catalogs can be added,
  edited, and removed, all from COSMIC Preferences → Online Library. No
  REST/PWA surface exists yet (see FEATURES.toml's acknowledged gap).

  @cosmic
  Scenario: Adding a custom catalog makes it appear in the list
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    When I add a catalog "My Library" with search URL "https://example.com/opds?q={searchTerms}"
    Then "My Library" appears in the list of online library catalogs

  @cosmic
  Scenario: Disabling a built-in catalog removes it from the enabled catalog list
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    When I disable the built-in catalog "standard_ebooks"
    Then "Standard Ebooks" no longer appears in the list of enabled online library catalogs
