@remotes_manage
Feature: Remote source management
  A read-flow instance lets a user register other instances as remote sources
  and remove them again. This is purely client-side bookkeeping — COSMIC's
  local DAO, the PWA's IndexedDB — with no REST surface (see the parity plan's
  gap analysis: "Remote-source management is client-side in both apps").

  Each driver maps "register"/"remove" onto its own natural shape: the PWA
  drives the real `/settings/sources` form and remove button; COSMIC inserts
  and deletes a `Remote` row directly via its DAO (no UI form to drive
  headlessly — same bypass `remotes_status` takes for "add"). Both converge on
  the same observable: does the source list reflect the change?

  @pwa @cosmic
  Scenario: Removing a remote source removes it from the list
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    And that server is registered as a remote source with user "alice" and passphrase "correct-horse"
    When I remove that remote source
    Then the list of remote sources is empty
