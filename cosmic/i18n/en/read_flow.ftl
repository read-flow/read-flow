# The keys in each section (separated by a comment) are sorted alphabetically

app-title = Read Flow
about = About
actions = Actions
context = Context
repository = Repository
scan = Scan
check-missing = Check for Missing Files
check-missing-dialog-title = Missing Files
check-missing-dialog-body = The following files are no longer accessible on disk:
check-missing-no-missing = No missing files found.
check-missing-purge = Purge
check-missing-cancel = Cancel
scan-progress-scanning = Scanning… { $discovered } discovered, { $processed } processed
scan-progress-completed = Last scan: { $discovered } discovered, { $processed } processed, { $errors } errors
view = View

view-options = View Options

# Preferences page
preferences-page-title = Preferences
preferences-back = ← Preferences
preferences-appearance-section = Appearance
preferences-appearance-section-description = Document viewer and display preferences
preferences-library-section = Library
preferences-library-section-description = Database file location
preferences-downloads-section = Downloads
preferences-downloads-section-description = Default folder for downloaded documents
preferences-scanning-section = Scanning
preferences-scanning-section-description = Auto-scan directories and supported file types
preferences-sources-section = Document Sources
preferences-sources-section-description = Remote servers to sync documents from
preferences-server-section = Server
preferences-server-section-description = Server access and authorized users
preferences-privacy-section = Privacy
preferences-privacy-section-description = Private mode and hidden tags

# Generic messages
generic-error = 🛑 Error: { $error }

# File details
document-details-close = Back
document-details-document-no-longer-accessible = Document no longer accessible
document-details-metadata-date = Date
document-details-metadata-identifier = Identifier
document-details-metadata-language = Language
document-details-metadata-publisher = Publisher
document-details-metadata-subject = Subject
document-details-no-covers = No covers available
document-details-no-sources = No sources available
document-details-select-cover = Select Cover
document-details-open-file = Open File
document-details-select-status = Select status
document-details-status = Status
document-details-tags = Tags

# User-edited document metadata
document-details-user-meta-section = Document Info
document-details-user-meta-edit = Edit document info
document-details-user-meta-save = Save
document-details-user-meta-cancel = Cancel
document-details-user-meta-type = Type
document-details-user-meta-type-none = Not set
document-details-user-meta-title = Title
document-details-user-meta-subtitle = Subtitle
document-details-user-meta-authors = Authors
document-details-user-meta-authors-add = Add author
document-details-user-meta-description = Description

# Tag editor
tag-editor-add = Add
tag-editor-loading-tags = Loading tags...
tag-editor-no-tags = No tags
tag-editor-remove-tag = Remove tag
tag-editor-select-tag = Select a tag

# Document details
document-details-copy-path = Copy path
document-details-delete-source = Delete source
document-details-delete-source-confirm-body = Are you sure you want to delete this source? This action cannot be undone.
document-details-delete-source-confirm-cancel = Cancel
document-details-delete-source-confirm-delete = Delete
document-details-delete-source-confirm-title = Delete Source
document-details-done-editing-sources = Done editing sources
document-details-download-to-local = Download to Local
document-details-edit-sources = Edit sources
document-details-send-to-missing = Send to
document-details-source-local = Local
document-details-sources = Formats
document-details-sync-to-all-sources = Sync to all sources
document-details-upload-to = Upload to { $host }

# Document list
document-list-all-sources = All sources
document-list-all-statuses = All statuses
document-list-clear-all-tag-filters = Clear All Tag Filters
document-list-clear-filter = Clear Filter
document-list-deselect-all = Deselect all
document-list-empty-description = Configure scan directories in Settings to let Read Flow discover your documents, then run a scan to add them to your library.
document-list-empty-title = No Documents Found
document-list-filter-by-source = Filter by Source
document-list-filter-by-status = Filter by Reading Status
document-list-filter-by-tags = Filter by Tags
document-list-filter-by-type = Filter by File Type
document-list-filtering = Filtering...
document-list-go-to-settings = Open Settings
document-list-keyboard-shortcuts = Keyboard shortcuts
document-list-loading = Loading
document-list-merge = Merge
document-list-merge-body = Choose the document that will keep its metadata. All file sources from the other documents will be moved to it.
document-list-merge-cancel = Cancel
document-list-merge-confirm = Merge
document-list-merge-title = Merge Documents
document-list-no-files = No files match the current filters
document-list-open-document-details = Open details
document-list-options-title = File List Options
document-list-pick-format-cancel = Cancel
document-list-pick-source-local = Local
document-list-pick-source-title = Open file
document-list-run-scan = Run Scan
document-list-search-mode = Search Mode
document-list-search-mode-fuzzy = Fuzzy
document-list-search-mode-regex = Regex
document-list-search-placeholder = Search files...
document-list-select-all = Select all
document-list-selection-count = Selected { $selected } of { $total }
document-list-shortcut-first-page = Go to first page
document-list-shortcut-last-page = Go to last page
document-list-shortcut-next-page = Go to next page
document-list-shortcut-previous-page = Go to previous page
document-list-shortcut-toggle-search-mode = Toggle search mode (Fuzzy / Regex)
document-list-sort-by = Sort by
document-list-sort-filename = Name
document-list-sort-size = Size
document-list-sort-status = Status
document-list-sort-title = Title
document-list-sort-type = Type

# Page messages
page-not-found = ⚠ Not found

# Language menu
language = Language
language-dutch = Nederlands
language-english = English
language-french = Français

# Pagination
pagination-first = First
pagination-last = Last
pagination-next = Next
pagination-page-of-total = Page { $page } of { $total }
pagination-prev = Previous

# Sources
sources-add-button = Add Source
sources-add-section-title = Add New Source
sources-edit-section-title = Edit Source
sources-add-unavailable-warning = This source appears unavailable. You can still add it — it will appear as unreachable until it becomes accessible.
sources-authorization-token = Authorization token
sources-authorization-token-placeholder = Enter authorization token...
sources-delete-confirm-body = Are you sure you want to delete this source? This action cannot be undone.
sources-delete-confirm-cancel = Cancel
sources-delete-confirm-delete = Delete
sources-delete-confirm-title = Delete Source
sources-empty-state = No remote sources configured
sources-error-close = Close
sources-error-title = An error occurred
sources-invalid-url = Not a valid URL
sources-loading-state-loading = Loading...
sources-loading-state-new = Initializing
sources-section-title = Remote Sources
sources-status-checking = Checking reachability...
sources-status-reachable = Reachable
sources-status-unknown = Status unknown
sources-status-unreachable = Unreachable
sources-url = Remote URL
sources-url-placeholder = Enter remote URL
sources-user-id = User ID
sources-user-id-placeholder = Enter user ID...

# Settings page
settings-add-directory = Add Directory
settings-back = ← Settings
settings-cancel-edit = Cancel
settings-client-download-folder = Download Folder
settings-client-download-folder-description = Folder where files pulled from remote sources are saved
settings-client-section = Client
settings-client-section-description = Configure the local folder where files downloaded from remote sources are saved.
settings-context-title = Privacy Settings
settings-database-location = Database Location
settings-database-location-description = The SQLite file that stores your document catalog and metadata. Restart the app after changing this path.
settings-database-section = Database
settings-database-section-description = Configure where your document catalog is stored.
settings-directory-action = Action
settings-directory-action-ignore = Ignore
settings-directory-action-ignore-label = Ignore
settings-directory-action-scan = Scan
settings-directory-action-scan-label = Scan
settings-directory-inherit = Inherit settings to subdirectories
settings-directory-path = Directory Path
settings-directory-tags = Tags
settings-edit-directory = Edit Directory
settings-epub-viewer = EPUB viewer preference
settings-epub-viewer-description = Choose which viewer opens EPUB files
settings-epub-viewer-external = External viewer (system default)
settings-epub-viewer-mupdf = MuPDF viewer
settings-epub-viewer-native = Native EPUB viewer (experimental)
settings-failed-to-load-tags = Failed to load tags
settings-no-directory-tags = No tags configured
settings-no-private-tags = No private tags configured
settings-page-title = Library Settings
settings-remove-directory = Remove Directory
settings-remove-directory-tag = Remove tag
settings-remove-private-tag = Remove private tag
settings-save = Save Settings
settings-save-directory = Save Directory
settings-save-error = Error saving settings
settings-saved = Settings saved successfully
settings-saving = Saving...
settings-scan-description = Read Flow indexes the documents it finds in the directories below. Add a directory, set its action to Scan, then hit Run Scan to populate your library.
settings-scan-directories-section = Scan Directories
settings-scan-dry-run = Dry Run Mode
settings-scan-dry-run-description = When enabled, scans discover files but do not add them to your library. Useful for previewing what would be indexed before committing.
settings-scan-file-types-description = Only files with an enabled extension will be picked up during a scan.
settings-scan-file-types-deselect-all = Deselect All
settings-scan-file-types-section = File Types
settings-scan-file-types-select-all = Select All
settings-scan-section = Scan
settings-scan-section-description = Configure which directories to index and which file types to include.
settings-select-directory-tag = Select a tag...
settings-select-private-tag = Select a tag...
settings-server-add-authorized-user = Add Authorized User
settings-server-authorized-users = Authorized Users
settings-server-authorized-users-description = Clients that may connect to this device's server. Each entry needs a user ID and a passphrase.
settings-server-description = Read Flow can serve your library over the network so other devices can connect and access your documents.
settings-server-download-folder = Download Folder
settings-server-download-folder-description = Where files sent to this device from remote clients are saved.
settings-server-edit-authorized-user = Edit Authorized User
settings-server-owner-role = Owner Role
settings-server-owner-role-description = Grants access to private content (required to request private mode from this server).
settings-server-passphrase = Passphrase
settings-server-passphrase-placeholder = Enter passphrase...
settings-server-section = Server
settings-server-section-description = Share your library over the network and manage which devices can connect.
settings-server-user-id = User ID
settings-server-user-id-placeholder = Enter user ID...
settings-ui-private-mode = Private Mode
settings-ui-private-mode-description = When enabled, documents that carry any private tag are hidden from the document list.
settings-ui-private-tags = Private Tags
settings-ui-private-tags-description = Documents tagged with any of these tags are hidden when Private Mode is on.
settings-ui-section = Privacy
settings-viewer-section = EPUB Viewer

# PDF viewer
pdf-viewer = PDF viewer
pdf-viewer-back = Back
pdf-viewer-document-details = Document Details
pdf-viewer-dual-pane = Two-page spread
pdf-viewer-dual-pane-auto = Auto
pdf-viewer-dual-pane-off = Off
pdf-viewer-dual-pane-on = On
pdf-viewer-epub-font-size = Font size (reflowable)
pdf-viewer-keyboard-shortcuts = Keyboard shortcuts
pdf-viewer-load-error = Failed to open PDF: { $error }
pdf-viewer-loading = Loading PDF...
pdf-viewer-no-local-source = No local source available for this document
pdf-viewer-shortcut-ctrl-scroll = Zoom with mouse
pdf-viewer-shortcut-fit-both = Fit width and height
pdf-viewer-shortcut-fit-height = Fit height
pdf-viewer-shortcut-fit-width = Fit width
pdf-viewer-shortcut-next-page = Next page
pdf-viewer-shortcut-previous-page = Previous page
pdf-viewer-shortcut-zoom-in = Zoom in
pdf-viewer-shortcut-zoom-out = Zoom out
pdf-viewer-shortcut-zoom-reset = Reset zoom (100%)
pdf-viewer-show-thumbnails = Show thumbnails
pdf-viewer-theme-colors = Theme colors
pdf-viewer-zoom = Zoom

# EPUB viewer
epub-viewer = EPUB viewer
epub-viewer-back = Back
epub-viewer-document-details = Document Details
epub-viewer-copy-code = Copy code
epub-viewer-display = Display
epub-viewer-dual-page = Two-page spread
epub-viewer-dual-page-auto = Auto
epub-viewer-dual-page-off = Off
epub-viewer-dual-page-on = On
epub-viewer-font = Font
epub-viewer-font-size = Font size
epub-viewer-image-viewer-title = Image
epub-viewer-image-zoom = Zoom
epub-viewer-image-zoom-fit-both = Fit width and height
epub-viewer-image-zoom-fit-height = Fit height
epub-viewer-image-zoom-fit-width = Fit width
epub-viewer-image-zoom-in = Zoom in
epub-viewer-image-zoom-out = Zoom out
epub-viewer-keyboard-shortcuts = Keyboard shortcuts
epub-viewer-load-error = Failed to open EPUB: { $error }
epub-viewer-loading = Loading EPUB...
epub-viewer-no-local-source = No local source available for this document
epub-viewer-page-margin = Page margin
epub-viewer-raw-html = Show raw HTML (debug)
epub-viewer-search = Search
epub-viewer-search-match-count = { $current } of { $total }
epub-viewer-search-next = Next match
epub-viewer-search-no-matches = No matches
epub-viewer-search-placeholder = Search in chapter...
epub-viewer-search-prev = Previous match
epub-viewer-shortcut-next-chapter = Next chapter
epub-viewer-shortcut-next-page = Next page
epub-viewer-shortcut-previous-chapter = Previous chapter
epub-viewer-shortcut-previous-page = Previous page
epub-viewer-view-paginated = Paginated view

# Dashboard
dashboard-page-title = Dashboard
dashboard-continue-reading = Continue Reading
dashboard-continue-reading-empty = Pick up where you left off
dashboard-continue-reading-empty-hint = Open a document to start tracking your reading progress.
dashboard-library-overview = Library Overview
dashboard-stat-documents = Documents
dashboard-stat-reading = Reading
dashboard-stat-completed = Completed
dashboard-format-breakdown = By Format
dashboard-sources = Sources
dashboard-quick-actions = Quick Actions
dashboard-all-documents = All Documents
dashboard-welcome-title = Welcome to Read Flow
dashboard-welcome-description = Your personal document library. Add some documents to get started.
dashboard-onboarding-step-scan-title = Configure Scan Directories
dashboard-onboarding-step-scan-description = Tell Read Flow where your documents live on disk.
dashboard-onboarding-step-run-title = Run Your First Scan
dashboard-onboarding-step-run-description = Discover and index all documents in your configured directories.
dashboard-onboarding-step-online-title = Browse Online Libraries
dashboard-onboarding-step-online-description = Download free books from Project Gutenberg and Standard Ebooks.
dashboard-onboarding-step-remote-title = Connect a Remote Source
dashboard-onboarding-step-remote-description = Sync with another Read Flow instance on your network.
dashboard-action-go = Go

# Online Libraries
online-library-back-to-filters = ← Back to filters
online-library-book-details = Book Details
online-library-catalog-all = All catalogs
online-library-catalog-section-title = Catalog
online-library-download = Download
online-library-downloaded = Added to library
online-library-downloading = Downloading…
online-library-empty-state = Enter a search term to find books in online catalogs
online-library-welcome-title = Discover Books Online
online-library-welcome-subtitle = Search free and open catalogs worldwide, then download directly to your library
online-library-hint-search-title = Search
online-library-hint-search-body = Find books by title, author, or keyword across all your connected catalogs
online-library-hint-download-title = Download
online-library-hint-download-body = Get books in EPUB, PDF, and other formats with one click
online-library-hint-library-title = Grow Your Library
online-library-hint-library-body = Downloaded books are automatically added to your local collection
online-library-layout-cards = Cards
online-library-layout-compact = Compact
online-library-layout-section-title = Layout
online-library-no-description = No description available
online-library-no-results = No results found
online-library-page-title = Online Libraries
online-library-search-button = Search
online-library-search-placeholder = Search books…
online-library-searching = Searching…

# Server page
server-log-page-title = Server
server-panel-title = Server control
server-start = Start server
server-stop = Stop server
server-restart = Restart
server-reload-config = Reload config
server-status-stopped = The server is stopped
server-status-stopped-detail = Start it to accept connections from your other devices.
server-status-starting = Starting the server…
server-status-running = The server is running
server-status-running-detail = Reachable at { $address }
server-status-failed = The server could not start
# Log viewer
server-log-min-level = Show
server-log-search = Search logs…
server-log-empty = Nothing to show yet
server-log-counts = { $errors } errors · { $warnings } warnings
server-log-details-title = Log entry
server-log-back = ← Back
server-log-select-hint = Select a log entry to see its message, fields, and spans.
server-log-detail-target = Source
server-log-detail-message = Message
server-log-detail-fields = Details
server-log-detail-spans = Context
log-level-trace = Trace and above
log-level-debug = Debug and above
log-level-info = Info and above
log-level-warn = Warnings and errors
log-level-error = Errors only
# Server preferences
settings-server-address = Bind address
settings-server-address-description = IP address the server listens on (e.g. 127.0.0.1 or 0.0.0.0)
settings-server-port = Port
settings-server-port-description = Port the server listens on (0 = pick a free port)
settings-server-start-on-launch = Start server on launch
settings-server-start-on-launch-description = Automatically start the server when the application opens
settings-server-restart-to-apply = Restart the server to apply the new address or port
