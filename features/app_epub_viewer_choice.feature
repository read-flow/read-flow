@app_epub_viewer_choice
Feature: EPUB viewer choice
  The COSMIC desktop app lets users choose which EPUB viewer backend to use:
  native parser, MuPDF, or an external application. The choice is persisted
  in memory during the session and written to the COSMIC config layer.

  @cosmic
  Scenario: Selecting MuPDF as EPUB viewer persists the preference
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    When I select "MuPdf" as the EPUB viewer
    Then "MuPdf" is the active EPUB viewer
