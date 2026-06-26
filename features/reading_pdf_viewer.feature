@reading_pdf_viewer
Feature: PDF viewer
  PDF documents can be opened for reading. The COSMIC desktop app uses the
  MuPDF renderer to display pages; the PWA uses PDF.js. Both surfaces show
  the document's pages once the file is loaded.

  @cosmic @pwa
  Scenario: Opening a PDF document shows its pages
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And a PDF document has been added to the library
    When I open the PDF document for reading
    Then the PDF pages are displayed
