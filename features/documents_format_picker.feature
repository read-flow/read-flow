@documents_format_picker
Feature: Document format picker
  When a document exists in multiple formats (e.g. both EPUB and PDF), the
  app offers a format picker so the user can choose which version to open.
  This is a client-side UI feature: REST has no equivalent surface.

  @cosmic @pwa
  Scenario: A document with multiple formats shows format choices
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And an EPUB and a PDF document have been added and merged
    Then multiple format choices are available for the merged document
