@theme_editor
Feature: Theme editor
  Users can customise the app's appearance. The chosen mode (System / Light /
  Dark) persists across sessions via localStorage.

  @pwa
  Scenario: Selecting dark mode persists the preference
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    When I set the theme mode to "Dark"
    Then the theme mode "dark" is persisted
