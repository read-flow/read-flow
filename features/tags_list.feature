@tags_list
Feature: Tag list
  The library tracks tags applied to documents and surfaces them so users can
  browse/filter by tag — this is the first scenario to need a seeded document,
  via the shared `features/fixtures/sample.epub` fixture (a minimal, valid
  EPUB the scanner can extract real metadata from, so a `Document` row gets
  created — see each driver's `seed_tagged_document` for how it gets in).

  @rest @cosmic @pwa
  Scenario: A tag applied to a document appears in the library's tag list
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And a document tagged "fiction" has been added to the library
    Then "fiction" appears in the library's list of tags
