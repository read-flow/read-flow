@documents_edit_metadata
Feature: Edit document metadata
  # Users can override the title (and other fields) extracted at scan time.
  # The PWA does this through the "Edit document info" form on the detail page.
  # REST/Cosmic use PUT /documents/<guid>/metadata directly.

  @rest @cosmic @pwa
  Scenario: Editing a document's title persists the change
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And a document has been added to the library
    When I set the document's title to "My Custom Title"
    Then the document's title is "My Custom Title"
