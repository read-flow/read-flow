@app_theme_overrides
Feature: Per-app theme overrides
  The COSMIC desktop app can be themed (dark/light, accent color, density,
  roundness, frosted glass, interface font) without changing the global
  COSMIC system settings. A light and a dark color profile are both
  configured at once, and the app switches between them live to match
  whichever mode the system is currently in. Overrides live in `[ui.theme]`
  of read-flow.toml; disabling the override returns to the system theme.

  @cosmic
  Scenario: The custom theme follows the system's dark/light mode
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    When I enable the custom app theme
    And I set the app accent color for "Dark" to "#ff0000"
    And I set the app accent color for "Light" to "#00aaff"
    And the system theme preference is "Dark"
    Then the effective app theme is dark
    And the effective accent color is "#ff0000"
    When the system theme preference is "Light"
    Then the effective app theme is light
    And the effective accent color is "#00aaff"

  @cosmic
  Scenario: Disabling the custom theme falls back to the system theme
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    When I enable the custom app theme
    And I disable the custom app theme
    Then the app follows the system theme
