@reading_image_viewer
Feature: Image viewer
  Raster and SVG images can be opened in the COSMIC desktop app's built-in
  image viewer. This surface is COSMIC-only — the PWA uses the browser's
  native rendering instead of a dedicated viewer page.

  @cosmic
  Scenario: Opening an image displays it in the viewer
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    When I open an image in the viewer
    Then the image is displayed
