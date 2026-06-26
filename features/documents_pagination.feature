@documents_pagination
Feature: Document list pagination
  The document list renders large collections efficiently. The COSMIC app uses
  a page-navigation component; the PWA uses virtual (windowed) scrolling. In
  both cases a freshly added document must appear in the initial view.

  @cosmic @pwa
  Scenario: A document appears on the first page of the library
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And a document has been added to the library
    Then "BDD Sample Book" appears on the first page of the document list
