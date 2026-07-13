@app_theme_overrides
Feature: Per-app theme overrides
  The COSMIC desktop app can be themed (dark/light, accent color, density,
  roundness, frosted glass, interface font) without changing the global
  COSMIC system settings. Overrides live in `[ui.theme]` of read-flow.toml
  and apply live; disabling the override returns to the system theme.

  @cosmic
  Scenario: Enabling a custom dark theme with a custom accent
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    When I enable the custom app theme
    And I set the app theme variant to "Dark"
    And I set the app accent color to "#ff0000"
    Then the effective app theme is dark
    And the effective accent color is "#ff0000"

  @cosmic
  Scenario: Disabling the custom theme falls back to the system theme
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    When I enable the custom app theme
    And I disable the custom app theme
    Then the app follows the system theme
