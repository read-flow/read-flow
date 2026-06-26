@reading_epub_viewer
Feature: EPUB viewer
  EPUB documents can be opened for reading. The native COSMIC viewer parses
  the document's spine and renders each chapter as blocks; the PWA uses
  epub.js. Both surfaces expose the document content once the file is loaded.

  @cosmic @pwa
  Scenario: Opening an EPUB document shows its content
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And a document has been added to the library
    When I open the document for reading
    Then the EPUB content is displayed
