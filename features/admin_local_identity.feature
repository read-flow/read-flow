@admin_local_identity
Feature: Local identity
  COSMIC's desktop app can bind its local (unauthenticated) database access to
  one of the server's authorized users, so reading progress and tags recorded
  locally are shared with that user's REST/PWA sessions instead of staying
  under the reserved, invisible `local` id. COSMIC-only: there is no REST or
  PWA UI for this (it configures how *this* desktop's own local access
  behaves), though the effect it produces is a REST-visible user.

  @cosmic
  Scenario: Binding local access to a user shares its locally-recorded progress with them
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And a document has been added to the library
    When I bind local access to user "alice" and record reading progress "chapter-3" at 42%
    Then "alice" is shown that reading progress "chapter-3" at 42%
