@remotes_private_mode
Feature: Remote private mode
  # Private mode is a per-surface client preference. REST and Cosmic test the
  # server-side setting (PUT /settings {private_mode: true} persists); the PWA
  # tests toggling the privateMode flag on a registered source. The setting
  # restricts document access to owner-role users when the x-private-mode
  # request header is active.

  @rest @cosmic @pwa
  Scenario: Private mode setting is saved and reported
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    When I enable private mode
    Then private mode is reported as enabled
