@sources_send_to_client
Feature: Send document to a client
  A document stored locally can be pushed to another source server (a
  "client"). The receiving server imports the file and indexes it. In the
  BDD harness the transfer target is the driver's own backend, which verifies
  the upload path end-to-end.

  @rest @cosmic @pwa
  Scenario: Sending a document to a client makes it available there
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    When I send a document to the server
    Then the document was accepted by the server
