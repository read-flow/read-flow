@online_library_download_import
Feature: Online library download and import
  A book discovered via the online library can be downloaded and imported
  into the local library. The server fetches the file from the download URL,
  scans it, and registers it as a document.

  @rest @cosmic @pwa
  Scenario: Importing an online book adds it to the library
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    When I import a book from the online library
    Then the book was imported successfully
