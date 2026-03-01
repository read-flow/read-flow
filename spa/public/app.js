// === Constants and Configuration ===
const CONFIG = {
  API_URL: "http://localhost:8000",
  PAGE_SIZE: 20,
  TAG_COLORS: {
    allowed: "bg-green-500 text-white",
    denied: "bg-red-500 text-white",
    default: "bg-blue-500 text-white",
  },
};

// === Authentication ===
const AUTH_STORAGE_KEY = "readflow_credentials";

function getStoredCredentials() {
  const stored = localStorage.getItem(AUTH_STORAGE_KEY);
  if (!stored) return null;
  try {
    return JSON.parse(stored);
  } catch {
    return null;
  }
}

function storeCredentials(username, password) {
  localStorage.setItem(
    AUTH_STORAGE_KEY,
    JSON.stringify({ username, password }),
  );
}

function clearCredentials() {
  localStorage.removeItem(AUTH_STORAGE_KEY);
}

function getAuthToken() {
  const creds = getStoredCredentials();
  if (!creds) return "";
  return "Basic " + btoa(`${creds.username}:${creds.password}`);
}

function showLoginModal(errorMessage) {
  const modal = document.getElementById("login-modal");
  const errorEl = document.getElementById("login-error");
  if (errorMessage) {
    errorEl.textContent = errorMessage;
    errorEl.classList.remove("hidden");
  } else {
    errorEl.classList.add("hidden");
  }
  modal.classList.remove("hidden");
}

function hideLoginModal() {
  document.getElementById("login-modal").classList.add("hidden");
}

function handleUnauthorized() {
  clearCredentials();
  showLoginModal("Invalid credentials. Please sign in again.");
}

async function apiFetch(url, options = {}) {
  const response = await fetch(url, {
    ...options,
    headers: {
      ...options.headers,
      Authorization: getAuthToken(),
    },
  });
  if (response.status === 401) {
    handleUnauthorized();
    throw new Error("Unauthorized");
  }
  return response;
}

// === Fuzzy Search Setup ===
// Initialize Fuse.js for fuzzy searching
let fuse = null;
const fuseOptions = {
  keys: ["name", "path"],
  threshold: 0.3,
  includeScore: true,
  includeMatches: true,
  minMatchCharLength: 2,
};

// === Type Definitions ===
/**
 * @typedef {Object} File
 * @property {number} id
 * @property {string} path
 * @property {string} type_
 * @property {number} size
 * @property {string} fingerprint
 * @property {string[]} tags
 * @property {string} status
 */

/**
 * @typedef {Object} FileState
 * @property {File[]} files
 * @property {File[]} filteredFiles
 * @property {File[]} currentView
 * @property {number} viewStart
 * @property {number} viewEnd
 * @property {boolean} loading
 * @property {string|null} error
 * @property {string} searchTerm
 * @property {string} sortField
 * @property {string} sortOrder
 */

// === State Management ===
let currentFile = null;
let allowedTags = new Set();
let deniedTags = new Set();
let allTags = new Set();

/**
 * @type {FileState}
 */
let fileState = {
  files: [],
  filteredFiles: [],
  currentView: [],
  viewStart: 0,
  viewEnd: CONFIG.PAGE_SIZE,
  loading: false,
  error: null,
  searchTerm: "",
  sortField: "name",
  sortOrder: "asc",
};

// === Search Functions ===
/**
 * Initialize fuzzy search with current files
 * @param {File[]} files
 */
function initializeFuzzySearch(files) {
  const searchableFiles = files.map((file) => {
    const { name } = parseFilePath(file.path);
    return {
      ...file,
      name: name,
    };
  });
  fuse = new Fuse(searchableFiles, fuseOptions);
}

/**
 * Perform fuzzy search on filenames
 * @param {string} searchTerm
 * @param {File[]} files
 * @returns {File[]}
 */
function fuzzySearchFiles(searchTerm, files) {
  if (!searchTerm.trim() || !fuse) {
    return files;
  }

  const results = fuse.search(searchTerm);
  return results.map((result) => result.item);
}

// === Utility Functions ===
/**
 * @param {string} bytes
 * @returns {string}
 */
function formatFileSize(bytes) {
  if (bytes === 0) return "0 Bytes";
  const k = 1024;
  const sizes = ["Bytes", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
}

/**
 * @param {Function} func
 * @param {number} wait
 * @returns {Function}
 */
function debounce(func, wait) {
  let timeout;
  return function executedFunction(...args) {
    const later = () => {
      clearTimeout(timeout);
      func(...args);
    };
    clearTimeout(timeout);
    timeout = setTimeout(later, wait);
  };
}

/**
 * @param {string} path
 * @returns {{name: string, directory: string}}
 */
function parseFilePath(path) {
  const fileName = path.split("/").pop();
  const fileDirectory = path.substring(0, path.lastIndexOf("/"));
  return { name: fileName, directory: fileDirectory };
}

// === API Functions ===
/**
 * @returns {Promise<void>}
 */
async function getAllTags() {
  try {
    const response = await apiFetch(`${CONFIG.API_URL}/files`);

    if (!response.ok) {
      throw new Error("Failed to fetch files");
    }

    const files = await response.json();

    // Clear the existing tags
    allTags.clear();

    // Add all tags from the files
    files.forEach((file) => {
      file.tags.forEach((tag) => allTags.add(tag));
    });
  } catch (error) {
    console.error("Error fetching tags:", error);
  }
}

/**
 * @param {number} fileId
 * @param {string} tag
 * @returns {Promise<void>}
 */
async function addFileTag(fileId, tag) {
  try {
    const response = await apiFetch(`${CONFIG.API_URL}/files/${fileId}/tags`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify([tag]),
    });

    if (!response.ok) {
      throw new Error("Failed to add tag");
    }

    // Update the tag list
    allTags.add(tag);
  } catch (error) {
    console.error("Error adding tag:", error);
    throw error;
  }
}

/**
 * @param {number} fileId
 * @param {string} tag
 * @returns {Promise<void>}
 */
async function removeFileTag(fileId, tag) {
  try {
    const response = await apiFetch(`${CONFIG.API_URL}/files/${fileId}/tags`, {
      method: "DELETE",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify([tag]),
    });

    if (!response.ok) {
      throw new Error("Failed to delete tag");
    }
  } catch (error) {
    console.error("Error deleting tag:", error);
    throw error;
  }
}

// === DOM Elements ===
const fileTableBody = document.getElementById("file-table-body");
const controlPanel = document.getElementById("control-panel");
const fileCountSpan = document.getElementById("file-count");
const filteredCountSpan = document.getElementById("filtered-count");
const detailsPane = document.getElementById("details-pane");
const detailsContent = document.getElementById("details-content");
const tableContainer = document.getElementById("table-container");
const closeDetailsPaneBtn = document.getElementById("close-details-pane");
const resizeHandle = document.getElementById("resize-handle");

// === File Operations ===
/**
 * @param {number} fileId
 * @param {string} fileName
 * @returns {Promise<void>}
 */
async function downloadFile(fileId, fileName) {
  try {
    const response = await apiFetch(
      `${CONFIG.API_URL}/files/${fileId}/download-as/${encodeURIComponent(fileName)}`,
    );

    if (!response.ok) {
      throw new Error("Failed to download file");
    }

    const blob = await response.blob();
    const url = window.URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = fileName;
    document.body.appendChild(a);
    a.click();
    window.URL.revokeObjectURL(url);
    document.body.removeChild(a);
  } catch (error) {
    console.error("Error downloading file:", error);
    alert("Failed to download file");
  }
}

// === UI Components ===
/**
 * @param {File[]} files
 * @param {Set} allowedTags
 * @param {Set} deniedTags
 * @returns {File[]}
 */
function filterFilesByTags(files, allowedTags, deniedTags) {
  return files.filter((file) => {
    const hasAllowedTags =
      allowedTags.size === 0 ||
      Array.from(allowedTags).every((tag) => file.tags.includes(tag));
    const hasDeniedTags =
      deniedTags.size === 0 ||
      Array.from(deniedTags).every((tag) => !file.tags.includes(tag));

    return hasAllowedTags && hasDeniedTags;
  });
}

/**
 * Combined filtering function for both tags and search term
 * @param {File[]} files
 * @param {Set} allowedTags
 * @param {Set} deniedTags
 * @param {string} searchTerm
 * @returns {File[]}
 */
function filterFiles(files, allowedTags, deniedTags, searchTerm) {
  let filteredFiles = files;

  // First apply fuzzy search if there's a search term
  if (searchTerm && searchTerm.trim()) {
    filteredFiles = fuzzySearchFiles(searchTerm, filteredFiles);
  }

  // Then apply tag filters on top of search results
  filteredFiles = filterFilesByTags(filteredFiles, allowedTags, deniedTags);

  return filteredFiles;
}

/**
 * @param {File} file
 * @returns {string}
 */
function createTagButtonHTML(file, tag) {
  const tagColor = allowedTags.has(tag)
    ? CONFIG.TAG_COLORS.allowed
    : deniedTags.has(tag)
      ? CONFIG.TAG_COLORS.denied
      : CONFIG.TAG_COLORS.default;

  return `
        <div class='relative'>
            <span class='inline-block ${tagColor} px-3 py-1 rounded-full text-sm font-medium'>
                ${tag}
            </span>
            <button
                class='absolute -right-1 -top-1 w-4 h-4 rounded-full bg-red-500 text-white hover:bg-red-600 transition-colors'
                onclick='deleteTag(${file.id}, '${tag}')'
                title='Remove tag'
            >
                <svg class='w-3 h-3' fill='none' stroke='currentColor' viewBox='0 0 24 24'>
                    <path stroke-linecap='round' stroke-linejoin='round' stroke-width='2' d='M6 18L18 6M6 6l12 12'></path>
                </svg>
            </button>
        </div>
    `;
}

/**
 * Creates the desktop-style control panel
 * @returns {HTMLDivElement}
 */
function createControlSection() {
  const controlSection = document.createElement("div");
  controlSection.className = "flex flex-wrap items-center gap-4";

  // Search section
  const searchContainer = document.createElement("div");
  searchContainer.className = "flex items-center space-x-2";

  const searchLabel = document.createElement("label");
  searchLabel.className = "text-sm font-medium text-gray-700";
  searchLabel.textContent = "Search:";

  const searchInputWrapper = document.createElement("div");
  searchInputWrapper.className = "relative";

  const searchInput = document.createElement("input");
  searchInput.type = "text";
  searchInput.id = "file-search";
  searchInput.placeholder = "Type to search files... (Press '/' to focus)";
  searchInput.className =
    "w-64 px-3 py-1 pl-8 text-sm border border-gray-300 rounded focus:ring-1 focus:ring-blue-500 focus:border-blue-500";
  searchInput.value = fileState.searchTerm;

  const searchIcon = document.createElement("div");
  searchIcon.className = "absolute left-2 top-1.5 text-gray-400 text-sm";
  searchIcon.innerHTML = "🔍";

  const clearButton = document.createElement("button");
  clearButton.className =
    "absolute right-2 top-1.5 text-gray-400 hover:text-gray-600 text-sm";
  clearButton.innerHTML = "✕";
  clearButton.onclick = () => {
    clearSearch();
    searchInput.focus();
  };

  searchInputWrapper.appendChild(searchIcon);
  searchInputWrapper.appendChild(searchInput);
  searchInputWrapper.appendChild(clearButton);

  // Search results display
  const searchResultsDiv = document.createElement("div");
  searchResultsDiv.id = "search-results-display";
  searchResultsDiv.className = "text-sm text-gray-600";

  searchContainer.appendChild(searchLabel);
  searchContainer.appendChild(searchInputWrapper);
  searchContainer.appendChild(searchResultsDiv);

  // Tag filters section
  const tagFiltersContainer = document.createElement("div");
  tagFiltersContainer.className = "flex items-center space-x-2 flex-wrap";

  const tagLabel = document.createElement("span");
  tagLabel.className = "text-sm font-medium text-gray-700";
  tagLabel.textContent = "Tags:";
  tagFiltersContainer.appendChild(tagLabel);

  // All tags buttons
  allTags.forEach((tag) => {
    const button = document.createElement("button");
    button.className = `px-2 py-1 text-xs rounded ${
      allowedTags.has(tag)
        ? "bg-green-500 text-white"
        : deniedTags.has(tag)
          ? "bg-red-500 text-white"
          : "bg-gray-200 text-gray-700 hover:bg-gray-300"
    } transition-colors`;
    button.textContent = tag;
    button.onclick = () => {
      if (allowedTags.has(tag)) {
        allowedTags.delete(tag);
        deniedTags.add(tag);
      } else if (deniedTags.has(tag)) {
        deniedTags.delete(tag);
      } else {
        allowedTags.add(tag);
      }
      updateFileList();
    };
    tagFiltersContainer.appendChild(button);
  });

  // Clear all button
  const clearAllButton = document.createElement("button");
  clearAllButton.className =
    "px-3 py-1 text-xs bg-gray-400 text-white rounded hover:bg-gray-500 transition-colors ml-2";
  clearAllButton.textContent = "Clear All";
  clearAllButton.onclick = () => {
    clearAll();
  };
  tagFiltersContainer.appendChild(clearAllButton);

  // Add debounced search functionality
  const debouncedSearch = debounce((searchTerm) => {
    fileState.searchTerm = searchTerm;
    updateFileList();
  }, 300);

  searchInput.addEventListener("input", (e) => {
    debouncedSearch(e.target.value);
  });

  // Keyboard shortcuts
  searchInput.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      clearSearch();
      searchInput.blur();
    } else if (
      e.key === "Enter" &&
      fileState.filteredFiles &&
      fileState.filteredFiles.length === 1
    ) {
      showFileDetailsPane(fileState.filteredFiles[0]);
    }
  });

  // Global keyboard shortcut
  document.addEventListener("keydown", (e) => {
    if (e.key === "/" && !e.ctrlKey && !e.metaKey && !e.altKey) {
      if (
        document.activeElement.tagName !== "INPUT" &&
        document.activeElement.tagName !== "TEXTAREA"
      ) {
        e.preventDefault();
        searchInput.focus();
      }
    }
  });

  controlSection.appendChild(searchContainer);
  controlSection.appendChild(tagFiltersContainer);
  return controlSection;
}

/**
 * Creates a table row for a file
 * @param {File} file
 * @returns {HTMLDivElement}
 */
function createFileRow(file) {
  const { name, directory } = parseFilePath(file.path);
  const row = document.createElement("div");
  row.className =
    "flex border-b border-gray-200 hover:bg-gray-50 cursor-pointer transition-colors";
  row.onclick = () => showFileDetailsPane(file);

  // Name column
  const nameCell = document.createElement("div");
  nameCell.className = "flex-1 px-4 py-3 border-r border-gray-200";

  // Highlight search terms if searching
  if (
    fileState.searchTerm.trim() &&
    name.toLowerCase().includes(fileState.searchTerm.toLowerCase())
  ) {
    const searchTerm = fileState.searchTerm.toLowerCase();
    const lowerName = name.toLowerCase();
    const startIndex = lowerName.indexOf(searchTerm);
    const endIndex = startIndex + searchTerm.length;
    const beforeMatch = name.substring(0, startIndex);
    const match = name.substring(startIndex, endIndex);
    const afterMatch = name.substring(endIndex);
    nameCell.innerHTML = `<span class="font-medium text-gray-900">${beforeMatch}<mark class="bg-yellow-200 px-1 rounded">${match}</mark>${afterMatch}</span>`;
  } else {
    nameCell.innerHTML = `<span class="font-medium text-gray-900">${name}</span>`;
  }

  // Type column
  const typeCell = document.createElement("div");
  typeCell.className =
    "w-32 px-4 py-3 text-sm text-gray-600 border-r border-gray-200";
  typeCell.textContent = file.type_ || "Unknown";

  // Size column
  const sizeCell = document.createElement("div");
  sizeCell.className =
    "w-24 px-4 py-3 text-sm text-gray-600 text-right border-r border-gray-200";
  sizeCell.textContent = formatFileSize(file.size || 0);

  // Directory column
  const dirCell = document.createElement("div");
  dirCell.className =
    "w-40 px-4 py-3 text-sm text-gray-500 truncate border-r border-gray-200";
  dirCell.textContent = directory;
  dirCell.title = directory; // Full path on hover

  // Tags column
  const tagsCell = document.createElement("div");
  tagsCell.className = "w-60 px-4 py-3";

  const tagsContainer = document.createElement("div");
  tagsContainer.className = "flex flex-wrap gap-1";

  file.tags.forEach((tag) => {
    const tagSpan = document.createElement("span");
    tagSpan.className = `px-2 py-1 text-xs rounded ${
      allowedTags.has(tag)
        ? "bg-green-100 text-green-800 border border-green-200"
        : deniedTags.has(tag)
          ? "bg-red-100 text-red-800 border border-red-200"
          : "bg-blue-100 text-blue-800 border border-blue-200"
    }`;
    tagSpan.textContent = tag;
    tagSpan.onclick = (e) => {
      e.stopPropagation();
      if (allowedTags.has(tag)) {
        allowedTags.delete(tag);
        deniedTags.add(tag);
      } else if (deniedTags.has(tag)) {
        deniedTags.delete(tag);
      } else {
        allowedTags.add(tag);
      }
      updateFileList();
    };
    tagsContainer.appendChild(tagSpan);
  });

  tagsCell.appendChild(tagsContainer);

  row.appendChild(nameCell);
  row.appendChild(typeCell);
  row.appendChild(sizeCell);
  row.appendChild(dirCell);
  row.appendChild(tagsCell);

  return row;
}

/**
 * Creates empty state row for the table
 * @returns {HTMLDivElement}
 */
function createEmptyStateRow() {
  const hasTagFilters = allowedTags.size > 0 || deniedTags.size > 0;
  const hasSearchTerm = fileState.searchTerm.trim();

  let title,
    message,
    buttons = "";

  if (hasSearchTerm && hasTagFilters) {
    title = "No files found";
    message = `No files match both your search term '${fileState.searchTerm}' and your tag filters.`;
    buttons = `
      <button onclick="clearSearch()" class="px-3 py-1 bg-blue-500 text-white rounded hover:bg-blue-600 transition-colors mr-2 text-sm">
        Clear Search
      </button>
      <button onclick="clearAll()" class="px-3 py-1 bg-gray-500 text-white rounded hover:bg-gray-600 transition-colors text-sm">
        Clear All Filters
      </button>
    `;
  } else if (hasSearchTerm) {
    title = "No files found";
    message = `Try adjusting your search term '${fileState.searchTerm}' or clear filters.`;
    buttons = `
      <button onclick="clearSearch()" class="px-3 py-1 bg-blue-500 text-white rounded hover:bg-blue-600 transition-colors text-sm">
        Clear Search
      </button>
    `;
  } else if (hasTagFilters) {
    title = "No files match your filters";
    message =
      "No files have the selected tag combination. Try adjusting your tag filters.";
    buttons = `
      <button onclick="clearAll()" class="px-3 py-1 bg-blue-500 text-white rounded hover:bg-blue-600 transition-colors text-sm">
        Clear Tag Filters
      </button>
    `;
  } else {
    title = "No files available";
    message = "Upload some files to get started.";
  }

  const row = document.createElement("div");
  row.className = "flex border-b border-gray-200";

  const emptyCell = document.createElement("div");
  emptyCell.className = "w-full px-4 py-12 text-center";
  emptyCell.innerHTML = `
    <div class="text-gray-400 text-4xl mb-4">📁</div>
    <h3 class="text-lg font-medium text-gray-900 mb-2">${title}</h3>
    <p class="text-gray-600 mb-4">${message}</p>
    ${buttons}
  `;

  row.appendChild(emptyCell);
  return row;
}

// === Helper Functions ===
/**
 * @param {number} fileId
 * @param {string} tag
 * @returns {Promise<void>}
 */
async function updateTagState(fileId, tag) {
  // Get the updated file details
  const updatedFileResponse = await apiFetch(
    `${CONFIG.API_URL}/files/${fileId}`,
  );

  if (!updatedFileResponse.ok) {
    throw new Error("Failed to fetch updated file details");
  }

  const updatedFile = await updatedFileResponse.json();

  // Remove the tag from allowed and denied lists if present
  if (allowedTags.has(tag)) {
    allowedTags.delete(tag);
  }
  if (deniedTags.has(tag)) {
    deniedTags.delete(tag);
  }

  // Update the details pane if it's currently open
  if (currentFile && currentFile.id === fileId) {
    if (detailsPane && !detailsPane.classList.contains("hidden")) {
      showFileDetailsPane(updatedFile);
    }
  }

  // Reload all tags from the server
  await getAllTags();

  // Update the file list to reflect changes
  updateFileList();
}

/**
 * @param {number} fileId
 * @returns {void}
 */
function handleTagKeyPress(event, fileId) {
  if (event.key === "Enter") {
    event.preventDefault();
    addTag(fileId);
  }
}

/**
 * @param {number} fileId
 * @returns {Promise<void>}
 */
async function addTag(fileId) {
  const input = document.getElementById(`tag-input-${fileId}`);
  const newTag = input.value.trim();

  if (!newTag) {
    alert("Please enter a tag");
    return;
  }

  try {
    await addFileTag(fileId, newTag);
    await updateTagState(fileId, newTag);

    // Clear the input
    input.value = "";
  } catch (error) {
    console.error("Error adding tag:", error);
    alert("Failed to add tag");
  }
}

/**
 * @param {number} fileId
 * @param {string} tag
 * @returns {Promise<void>}
 */
async function deleteTag(fileId, tag) {
  try {
    await removeFileTag(fileId, tag);
    await updateTagState(fileId, tag);
  } catch (error) {
    console.error("Error deleting tag:", error);
    alert("Failed to delete tag");
  }
}

/**
 * Shows file details in the collapsible right pane
 * @param {File} file
 * @returns {void}
 */
function showFileDetailsPane(file) {
  currentFile = file;

  if (!detailsPane || !detailsContent) {
    console.error("Details pane elements not found");
    return;
  }

  const fileName = file.path.split("/").pop();
  const fileDirectory = file.path.substring(0, file.path.lastIndexOf("/"));

  // Show the details pane and resize handle
  detailsPane.classList.remove("hidden");
  if (resizeHandle) {
    resizeHandle.classList.remove("hidden");
    // Add hint animation for first-time users
    resizeHandle.classList.add("hint");
    setTimeout(() => {
      resizeHandle.classList.remove("hint");
    }, 6000);
  }

  // Adjust table container width
  if (tableContainer) {
    tableContainer.style.marginRight = "0";
  }

  // Populate the details content
  detailsContent.innerHTML = `
    <div class="space-y-6">
      <!-- File Header -->
      <div>
        <h3 class="text-xl font-bold text-gray-900 mb-2">${fileName}</h3>
        <p class="text-sm text-gray-600">${fileDirectory}</p>
      </div>

      <!-- Basic Info -->
      <div>
        <h4 class="text-sm font-semibold text-gray-700 uppercase tracking-wide mb-3">Basic Info</h4>
        <div class="space-y-2 text-sm">
          <div class="flex justify-between py-1">
            <span class="text-gray-600">Type:</span>
            <span class="font-medium">${file.type_ || "Unknown"}</span>
          </div>
          <div class="flex justify-between py-1">
            <span class="text-gray-600">Size:</span>
            <span class="font-medium">${formatFileSize(file.size || 0)}</span>
          </div>
          <div class="flex justify-between py-1">
            <span class="text-gray-600">Status:</span>
            <span class="font-medium">${file.status || "Unknown"}</span>
          </div>
          <div class="flex justify-between py-1">
            <span class="text-gray-600">ID:</span>
            <span class="font-medium">#${file.id}</span>
          </div>
        </div>
      </div>

      <!-- File Path -->
      <div>
        <h4 class="text-sm font-semibold text-gray-700 uppercase tracking-wide mb-3">File Path</h4>
        <div class="bg-gray-50 p-3 rounded text-sm">
          <code class="break-all">${file.path}</code>
        </div>
      </div>

      <!-- Actions -->
      <div>
        <h4 class="text-sm font-semibold text-gray-700 uppercase tracking-wide mb-3">Actions</h4>
        <button
          class="w-full bg-green-600 hover:bg-green-700 text-white px-4 py-2 rounded text-sm font-medium transition-colors"
          onclick="downloadFile(${file.id}, '${fileName}')"
        >
          Download File
        </button>
      </div>

      <!-- Tags Section -->
      <div>
        <h4 class="text-sm font-semibold text-gray-700 uppercase tracking-wide mb-3">Tags</h4>
        <div class="flex flex-wrap gap-1 mb-3" data-file-id="${file.id}">
          ${file.tags
            .map(
              (tag) => `
            <span class="px-2 py-1 text-xs rounded ${
              allowedTags.has(tag)
                ? "bg-green-100 text-green-800 border border-green-200"
                : deniedTags.has(tag)
                  ? "bg-red-100 text-red-800 border border-red-200"
                  : "bg-blue-100 text-blue-800 border border-blue-200"
            }">${tag}</span>
          `,
            )
            .join("")}
        </div>

        <!-- Add Tag Form -->
        <div class="space-y-2">
          <input
            type="text"
            placeholder="Enter new tag"
            class="w-full px-3 py-2 text-sm border border-gray-300 rounded focus:ring-1 focus:ring-blue-500 focus:border-blue-500"
            id="tag-input-${file.id}"
            onkeypress="handleTagKeyPress(event, ${file.id})"
          >
          <button
            class="w-full bg-blue-600 hover:bg-blue-700 text-white px-3 py-2 rounded text-sm font-medium transition-colors"
            onclick="addTag(${file.id})"
          >
            Add Tag
          </button>
        </div>
      </div>

      <!-- Technical Details -->
      <div>
        <h4 class="text-sm font-semibold text-gray-700 uppercase tracking-wide mb-3">Technical Details</h4>
        <div>
          <p class="text-xs text-gray-600 mb-2">Fingerprint:</p>
          <div class="bg-gray-50 p-3 rounded text-xs">
            <code class="break-all">${file.fingerprint || "N/A"}</code>
          </div>
        </div>
      </div>
    </div>
  `;
}

/**
 * Hides the file details pane
 * @returns {void}
 */
function hideFileDetailsPane() {
  if (!detailsPane || !tableContainer) {
    return;
  }

  // Hide the details pane and resize handle
  detailsPane.classList.add("hidden");
  if (resizeHandle) {
    resizeHandle.classList.add("hidden");
  }

  // Reset table container
  tableContainer.style.marginRight = "";

  // Clear current file
  currentFile = null;

  // Clear details content
  if (detailsContent) {
    detailsContent.innerHTML = "";
  }
}

/**
 * Legacy function for backward compatibility
 * @param {File} file
 * @returns {void}
 */
function showFileDetails(file) {
  showFileDetailsPane(file);
}

/**
 * @returns {Promise<void>}
 */
async function updateFileList() {
  try {
    // Show loading state
    fileState.loading = true;
    fileState.error = null;

    // Initialize details pane close button if not already done
    if (closeDetailsPaneBtn && !closeDetailsPaneBtn.onclick) {
      closeDetailsPaneBtn.onclick = hideFileDetailsPane;
    }

    const response = await apiFetch(`${CONFIG.API_URL}/files`);

    if (!response.ok) {
      throw new Error(
        `Failed to fetch files: ${response.status} ${response.statusStatus}`,
      );
    }

    const files = await response.json();

    // Initialize fuzzy search with current files
    initializeFuzzySearch(files);

    // Filter files by both tags and search term
    const filteredFiles = filterFiles(
      files,
      allowedTags,
      deniedTags,
      fileState.searchTerm,
    );

    // Store filtered files for global access
    fileState.filteredFiles = filteredFiles;

    // Clear loading state
    fileState.loading = false;

    // Update file counts in header
    if (fileCountSpan) {
      const fileText = files.length === 1 ? "file" : "files";
      fileCountSpan.textContent = `${files.length} ${fileText}`;
    }
    if (filteredCountSpan) {
      const showingText = filteredFiles.length === 1 ? "showing" : "showing";
      filteredCountSpan.textContent = `${filteredFiles.length} ${showingText}`;
    }

    // Update control panel
    controlPanel.innerHTML = "";
    const controlSection = createControlSection();
    controlPanel.appendChild(controlSection);

    // Update search results display
    const searchResultsDisplay = document.getElementById(
      "search-results-display",
    );
    if (searchResultsDisplay) {
      if (fileState.searchTerm.trim()) {
        const resultText = filteredFiles.length === 1 ? "result" : "results";
        const hasTagFilters = allowedTags.size > 0 || deniedTags.size > 0;
        const filterText = hasTagFilters ? " (with tag filters)" : "";
        searchResultsDisplay.textContent = `${filteredFiles.length} ${resultText} found${filterText}`;
      } else {
        searchResultsDisplay.textContent = "";
      }
    }

    // Update table body
    fileTableBody.innerHTML = "";

    if (filteredFiles.length === 0) {
      // Show empty state row
      const emptyRow = createEmptyStateRow();
      fileTableBody.appendChild(emptyRow);
    } else {
      // Show file rows
      filteredFiles.forEach((file) => {
        const fileRow = createFileRow(file);
        fileTableBody.appendChild(fileRow);
      });
    }
  } catch (error) {
    console.error("Error updating file list:", error);
    fileState.loading = false;
    fileState.error = error.message;

    // Show error state
    fileTableBody.innerHTML = "";
    const errorRow = document.createElement("div");
    errorRow.className = "flex border-b border-gray-200";
    const errorCell = document.createElement("div");
    errorCell.className = "w-full px-4 py-12 text-center";
    errorCell.innerHTML = `
      <div class="text-red-400 text-4xl mb-4">⚠️</div>
      <h3 class="text-lg font-medium text-gray-900 mb-2">Failed to load files</h3>
      <p class="text-gray-600 mb-4">${error.message}</p>
      <button onclick="updateFileList()" class="px-4 py-2 bg-blue-500 text-white rounded hover:bg-blue-600">
        Try Again
      </button>
    `;
    errorRow.appendChild(errorCell);
    fileTableBody.appendChild(errorRow);
  }
}

// === Global Helper Functions ===
/**
 * Global function to clear search and refresh file list
 * Used by buttons in the UI that need to reset search state
 */
window.clearSearch = function () {
  console.log("clearSearch called");
  const searchInput = document.getElementById("file-search");
  if (searchInput) {
    searchInput.value = "";
    console.log("Search input cleared");
  } else {
    console.log("Search input not found");
  }
  fileState.searchTerm = "";
  console.log("Search term reset to:", fileState.searchTerm);

  // Force update the search results display
  const searchResultsDisplay = document.getElementById(
    "search-results-display",
  );
  if (searchResultsDisplay) {
    searchResultsDisplay.innerHTML = "";
  }

  updateFileList();
  console.log("File list updated");
};

/**
 * Global function to clear everything (search and tag filters)
 * Used by the "Clear All" button
 */
window.clearAll = function () {
  console.log("clearAll called");

  // Clear search
  const searchInput = document.getElementById("file-search");
  if (searchInput) {
    searchInput.value = "";
  }
  fileState.searchTerm = "";

  // Clear search results display
  const searchResultsDisplay = document.getElementById(
    "search-results-display",
  );
  if (searchResultsDisplay) {
    searchResultsDisplay.innerHTML = "";
  }

  // Clear tag filters
  allowedTags.clear();
  deniedTags.clear();

  updateFileList();
  console.log("All filters cleared");
};

// Make sure the functions are globally accessible
if (typeof window !== "undefined") {
  window.clearSearch = window.clearSearch;
  window.clearAll = window.clearAll;
}

// === Resize Functionality ===
let isResizing = false;
let startX = 0;
let startWidth = 0;
const PANE_MIN_WIDTH = 300;
const PANE_MAX_WIDTH_PX = 800;
const PANE_MAX_WIDTH_VW = 0.4; // 40% of viewport width
const PANE_DEFAULT_WIDTH = 384;

/**
 * Initialize resize functionality for the details pane
 */
function initializeResize() {
  if (!resizeHandle || !detailsPane) {
    return;
  }

  // Mouse down on resize handle
  resizeHandle.addEventListener("mousedown", startResize);

  // Double-click to reset to default width
  resizeHandle.addEventListener("dblclick", resetPaneWidth);
}

/**
 * Start resizing the details pane
 */
function startResize(e) {
  isResizing = true;
  startX = e.clientX;
  startWidth = parseInt(window.getComputedStyle(detailsPane).width, 10);

  // Add visual feedback
  document.body.classList.add("resizing");
  resizeHandle.classList.add("active");

  // Disable transitions during resize
  detailsPane.style.transition = "none";

  // Add event listeners
  document.addEventListener("mousemove", handleResize);
  document.addEventListener("mouseup", stopResize);
  document.addEventListener("keydown", handleResizeKeydown);

  e.preventDefault();
}

/**
 * Handle mouse move during resize
 */
function handleResize(e) {
  if (!isResizing) return;

  const deltaX = e.clientX - startX;
  const newWidth = startWidth - deltaX;
  const maxWidth = Math.min(
    PANE_MAX_WIDTH_PX,
    window.innerWidth * PANE_MAX_WIDTH_VW,
  );
  const clampedWidth = Math.max(PANE_MIN_WIDTH, Math.min(maxWidth, newWidth));

  detailsPane.style.width = clampedWidth + "px";

  // Store the width for persistence
  localStorage.setItem("detailsPaneWidth", clampedWidth);

  e.preventDefault();
}

/**
 * Stop resizing
 */
function stopResize() {
  if (!isResizing) return;

  isResizing = false;

  // Remove visual feedback
  document.body.classList.remove("resizing");
  resizeHandle.classList.remove("active");

  // Re-enable transitions
  detailsPane.style.transition = "";

  // Remove event listeners
  document.removeEventListener("mousemove", handleResize);
  document.removeEventListener("mouseup", stopResize);
  document.removeEventListener("keydown", handleResizeKeydown);
}

/**
 * Handle keyboard events during resize
 */
function handleResizeKeydown(e) {
  if (e.key === "Escape") {
    stopResize();
  }
}

/**
 * Reset pane width to default
 */
function resetPaneWidth() {
  if (detailsPane) {
    detailsPane.style.width = PANE_DEFAULT_WIDTH + "px";
    localStorage.setItem("detailsPaneWidth", PANE_DEFAULT_WIDTH);
  }
}

/**
 * Load saved pane width from localStorage
 */
function loadPaneWidth() {
  const savedWidth = localStorage.getItem("detailsPaneWidth");
  if (savedWidth && detailsPane) {
    const width = parseInt(savedWidth, 10);
    const maxWidth = Math.min(
      PANE_MAX_WIDTH_PX,
      window.innerWidth * PANE_MAX_WIDTH_VW,
    );
    if (width >= PANE_MIN_WIDTH && width <= maxWidth) {
      detailsPane.style.width = width + "px";
    } else {
      // Reset to default if saved width is invalid for current screen
      detailsPane.style.width = Math.min(PANE_DEFAULT_WIDTH, maxWidth) + "px";
    }
  }
}

/**
 * Handle window resize to maintain proper pane constraints
 */
function handleWindowResize() {
  if (detailsPane && !detailsPane.classList.contains("hidden")) {
    const currentWidth = parseInt(
      window.getComputedStyle(detailsPane).width,
      10,
    );
    const maxWidth = Math.min(
      PANE_MAX_WIDTH_PX,
      window.innerWidth * PANE_MAX_WIDTH_VW,
    );

    // If current width exceeds new maximum, resize to fit
    if (currentWidth > maxWidth) {
      detailsPane.style.width = maxWidth + "px";
      localStorage.setItem("detailsPaneWidth", maxWidth);
    }
  }
}

// === Initialization ===
function initializeApp() {
  getAllTags().then(() => {
    updateFileList();

    // Set up details pane close button
    if (closeDetailsPaneBtn) {
      closeDetailsPaneBtn.onclick = hideFileDetailsPane;
    }

    // Initialize resize functionality
    initializeResize();

    // Load saved pane width
    loadPaneWidth();

    // Handle window resize
    window.addEventListener("resize", debounce(handleWindowResize, 250));
  });
}

document.getElementById("login-form").addEventListener("submit", (e) => {
  e.preventDefault();
  const username = document.getElementById("login-username").value.trim();
  const password = document.getElementById("login-password").value;
  storeCredentials(username, password);
  hideLoginModal();
  initializeApp();
});

if (getStoredCredentials()) {
  hideLoginModal();
  initializeApp();
} else {
  showLoginModal();
}
