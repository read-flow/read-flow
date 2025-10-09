// === Constants and Configuration ===
const CONFIG = {
  API_URL: "http://localhost:8000",
  AUTH_TOKEN: "bearer secret",
  PAGE_SIZE: 20,
  TAG_COLORS: {
    allowed: "bg-green-500 text-white",
    denied: "bg-red-500 text-white",
    default: "bg-blue-500 text-white",
  },
};

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
    const response = await fetch(`${CONFIG.API_URL}/files`, {
      headers: {
        Authorization: CONFIG.AUTH_TOKEN,
      },
    });

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
    const response = await fetch(`${CONFIG.API_URL}/files/${fileId}/tags`, {
      method: "POST",
      headers: {
        Authorization: CONFIG.AUTH_TOKEN,
        "Content-Type": "application/json",
      },
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
    const response = await fetch(`${CONFIG.API_URL}/files/${fileId}/tags`, {
      method: "DELETE",
      headers: {
        Authorization: CONFIG.AUTH_TOKEN,
        "Content-Type": "application/json",
      },
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
const fileListDiv = document.getElementById("file-list");

// === File Operations ===
/**
 * @param {number} fileId
 * @param {string} fileName
 * @returns {Promise<void>}
 */
async function downloadFile(fileId, fileName) {
  try {
    const response = await fetch(
      `${CONFIG.API_URL}/files/${fileId}/download-as/${encodeURIComponent(fileName)}`,
      {
        headers: {
          Authorization: CONFIG.AUTH_TOKEN,
        },
      },
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
 * @returns {HTMLDivElement}
 */
function createControlSection() {
  const controlSection = document.createElement("div");
  controlSection.className = "bg-gray-50 p-4 mb-6 rounded-lg shadow-sm";

  // Create search input section
  const searchSection = document.createElement("div");
  searchSection.className = "mb-4";

  const searchLabel = document.createElement("label");
  searchLabel.className = "block text-sm font-medium text-gray-700 mb-2";
  searchLabel.textContent = "Search files by name:";

  const searchInputContainer = document.createElement("div");
  searchInputContainer.className = "relative";

  // Add search icon
  const searchIcon = document.createElement("div");
  searchIcon.className = "absolute left-3 top-2.5 text-gray-400";
  searchIcon.innerHTML = "🔍";
  searchInputContainer.appendChild(searchIcon);

  const searchInput = document.createElement("input");
  searchInput.type = "text";
  searchInput.id = "file-search";
  searchInput.placeholder =
    "Type to search filenames... (Press " / " to focus, Esc to clear)";
  searchInput.className =
    "w-full px-4 py-2 pl-10 border border-gray-300 rounded-lg focus:ring-2 focus:ring-blue-500 focus:border-blue-500";
  searchInput.value = fileState.searchTerm;

  const clearSearchButton = document.createElement("button");
  clearSearchButton.className =
    "absolute right-2 top-2 text-gray-400 hover:text-gray-600";
  clearSearchButton.innerHTML = "✕";
  clearSearchButton.onclick = () => {
    clearSearch();
    searchInput.focus();
  };

  searchInputContainer.appendChild(searchInput);
  searchInputContainer.appendChild(clearSearchButton);
  searchSection.appendChild(searchLabel);
  searchSection.appendChild(searchInputContainer);

  // Add search results display
  const searchResultsDiv = document.createElement("div");
  searchResultsDiv.id = "search-results-display";
  searchResultsDiv.className = "mt-2";
  searchSection.appendChild(searchResultsDiv);

  // Add debounced search functionality
  const debouncedSearch = debounce((searchTerm) => {
    fileState.searchTerm = searchTerm;
    updateFileList();
  }, 300);

  searchInput.addEventListener("input", (e) => {
    debouncedSearch(e.target.value);
  });

  // Add keyboard shortcuts
  searchInput.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      clearSearch();
      searchInput.blur();
    } else if (
      e.key === "Enter" &&
      filteredFiles &&
      filteredFiles.length === 1
    ) {
      // If only one result, open it
      showFileDetails(filteredFiles[0]);
    }
  });

  // Focus search input when '/' key is pressed
  document.addEventListener("keydown", (e) => {
    if (e.key === "/" && !e.ctrlKey && !e.metaKey && !e.altKey) {
      // Only if not focused on an input element
      if (
        document.activeElement.tagName !== "INPUT" &&
        document.activeElement.tagName !== "TEXTAREA"
      ) {
        e.preventDefault();
        searchInput.focus();
      }
    }
  });

  controlSection.appendChild(searchSection);

  // Create tag filters container
  const tagFiltersContainer = document.createElement("div");
  tagFiltersContainer.className = "mb-4 flex flex-wrap gap-2";

  // Add buttons for each tag
  allTags.forEach((tag) => {
    const button = document.createElement("button");
    button.className = `px-4 py-2 rounded-lg ${
      allowedTags.has(tag)
        ? "bg-green-500 text-white"
        : deniedTags.has(tag)
          ? "bg-red-500 text-white"
          : "bg-gray-200 text-gray-700"
    } hover:bg-blue-600 transition-colors`;
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

  // Create selected tags container
  const selectedTagsContainer = document.createElement("div");
  selectedTagsContainer.className = "mb-4";

  // Create allowed tags section
  const allowedSection = document.createElement("div");
  allowedSection.className = "mb-4";

  const allowedLabel = document.createElement("div");
  allowedLabel.className = "text-sm font-medium text-gray-700 mb-2";
  allowedLabel.textContent = "Show files with all of these tags:";
  allowedSection.appendChild(allowedLabel);

  const allowedTagsContainer = document.createElement("div");
  allowedTagsContainer.className = "flex flex-wrap gap-2";

  Array.from(allowedTags).forEach((tag) => {
    const tagButton = document.createElement("button");
    tagButton.className =
      "px-3 py-1 rounded-full text-sm font-medium bg-green-500 text-white hover:bg-green-600 transition-colors";
    tagButton.textContent = tag;

    tagButton.onclick = (e) => {
      e.stopPropagation();
      allowedTags.delete(tag);
      updateFileList();
    };

    allowedTagsContainer.appendChild(tagButton);
  });

  allowedSection.appendChild(allowedTagsContainer);
  selectedTagsContainer.appendChild(allowedSection);

  // Create denied tags section
  const deniedSection = document.createElement("div");

  const deniedLabel = document.createElement("div");
  deniedLabel.className = "text-sm font-medium text-gray-700 mb-2";
  deniedLabel.textContent = "Hide files with any of these tags:";
  deniedSection.appendChild(deniedLabel);

  const deniedTagsContainer = document.createElement("div");
  deniedTagsContainer.className = "flex flex-wrap gap-2";

  Array.from(deniedTags).forEach((tag) => {
    const tagButton = document.createElement("button");
    tagButton.className =
      "px-3 py-1 rounded-full text-sm font-medium bg-red-500 text-white hover:bg-red-600 transition-colors";
    tagButton.textContent = tag;

    tagButton.onclick = (e) => {
      e.stopPropagation();
      deniedTags.delete(tag);
      updateFileList();
    };

    deniedTagsContainer.appendChild(tagButton);
  });

  deniedSection.appendChild(deniedTagsContainer);
  selectedTagsContainer.appendChild(deniedSection);

  // Add clear all button at the bottom
  const clearAllButton = document.createElement("button");
  clearAllButton.className =
    "px-4 py-2 rounded-lg bg-gray-200 text-gray-700 hover:bg-gray-300 transition-colors";
  clearAllButton.textContent = "Clear All";
  clearAllButton.onclick = () => {
    clearAll();
  };
  selectedTagsContainer.appendChild(clearAllButton);

  controlSection.appendChild(tagFiltersContainer);
  controlSection.appendChild(selectedTagsContainer);
  return controlSection;
}

// === Helper Functions ===
/**
 * @param {number} fileId
 * @param {string} tag
 * @returns {Promise<void>}
 */
async function updateTagState(fileId, tag) {
  // Get the updated file details
  const updatedFileResponse = await fetch(`${CONFIG.API_URL}/files/${fileId}`, {
    headers: {
      Authorization: CONFIG.AUTH_TOKEN,
    },
  });

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

  // Update the details card if it's currently open
  if (currentFile && currentFile.id === fileId) {
    const detailsContainer = document.getElementById("file-details");
    if (detailsContainer) {
      detailsContainer.remove();
      showFileDetails(updatedFile);
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
 * @param {File} file
 * @returns {void}
 */
function showFileDetails(file) {
  currentFile = file;

  const fileName = file.path.split("/").pop();
  const fileDirectory = file.path.substring(0, file.path.lastIndexOf("/"));

  // Create details container if it doesn't exist
  let detailsContainer = document.getElementById("file-details");
  if (!detailsContainer) {
    detailsContainer = document.createElement("div");
    detailsContainer.id = "file-details";
    detailsContainer.className =
      "fixed inset-0 bg-gray-900 bg-opacity-50 flex items-center justify-center z-50";
    document.body.appendChild(detailsContainer);
  }

  const modalContent = document.createElement("div");
  modalContent.className =
    "bg-white rounded-lg p-8 w-full max-w-4xl mx-4 flex flex-col";

  modalContent.innerHTML = `
        <div class='flex justify-between items-start mb-6'>
            <h2 class='text-2xl font-bold'>${fileName}</h2>
            <button
                class='text-gray-400 hover:text-gray-500'
                onclick='document.getElementById('file-details').remove()'
            >
                <svg class='w-6 h-6' fill='none' stroke='currentColor' viewBox='0 0 24 24'>
                    <path stroke-linecap='round' stroke-linejoin='round' stroke-width='2' d='M6 18L18 6M6 6l12 12'></path>
                </svg>
            </button>
        </div>

        <div class='grid grid-cols-1 md:grid-cols-2 gap-6 mb-8'>
            <!-- Left column - Basic Info -->
            <div>
                <h3 class='text-lg font-semibold mb-4'>Basic Info</h3>
                <div class='space-y-2'>
                    <div class='flex justify-between'>
                        <span class='text-gray-600'>Type</span>
                        <span class='font-medium'>${file.type_}</span>
                    </div>
                    <div class='flex justify-between'>
                        <span class='text-gray-600'>Size</span>
                        <span class='font-medium'>${formatFileSize(file.size)}</span>
                    </div>
                    <div class='flex justify-between'>
                        <span class='text-gray-600'>Status</span>
                        <span class='font-medium'>${file.status}</span>
                    </div>
                    <div class='flex justify-between'>
                        <span class='text-gray-600'>ID</span>
                        <span class='font-medium'>#${file.id}</span>
                    </div>
                </div>
            </div>

            <!-- Right column - File Path -->
            <div>
                <h3 class='text-lg font-semibold mb-4'>File Details</h3>
                <div class='space-y-4'>
                    <div>
                        <h4 class='text-sm font-medium text-gray-600 mb-1'>File Path</h4>
                        <div class='bg-gray-50 p-3 rounded'>
                            <code class='text-sm'>${file.path}</code>
                        </div>
                    </div>
                </div>
            </div>
        </div>

        <!-- Actions -->
        <div class='mb-8'>
            <h3 class='text-lg font-semibold mb-4'>Actions</h3>
            <button
                class='w-full bg-green-500 text-white px-4 py-2 rounded-lg hover:bg-green-600 transition-colors'
                onclick='downloadFile(${file.id}, '${fileName}')'
            >
                Download
            </button>
        </div>

        <!-- Tags Section -->
        <div class='mb-8'>
            <h3 class='text-lg font-semibold mb-4'>Tags</h3>
            <div class='flex flex-wrap gap-2' data-file-id='${file.id}'>
                ${file.tags.map((tag) => createTagButtonHTML(file, tag)).join("")}
            </div>

            <!-- Add Tag Form -->
            <div class='mt-4'>
                <div class='flex gap-2'>
                    <input
                        type='text'
                        placeholder='Enter new tag'
                        class='flex-1 px-4 py-2 rounded-lg border border-gray-300 focus:outline-none focus:ring-2 focus:ring-blue-500'
                        id='tag-input-${file.id}'
                        onkeypress='handleTagKeyPress(event, ${file.id})'
                    >
                    <button
                        class='bg-blue-500 text-white px-4 py-2 rounded-lg hover:bg-blue-600 transition-colors'
                        onclick='addTag(${file.id})'
                    >
                        Add Tag
                    </button>
                </div>
            </div>
        </div>

        <!-- Fingerprint Section -->
        <div class='border-t pt-6'>
            <h3 class='text-lg font-semibold mb-4'>Technical Details</h3>
            <div class='space-y-4'>
                <div>
                    <h4 class='text-sm font-medium text-gray-600 mb-1'>Fingerprint</h4>
                    <div class='bg-gray-50 p-3 rounded'>
                        <code class='text-sm'>${file.fingerprint}</code>
                    </div>
                </div>
            </div>
        </div>
    `;

  detailsContainer.onclick = (e) => {
    if (e.target === detailsContainer) {
      detailsContainer.remove();
      currentFile = null;
    }
  };

  detailsContainer.appendChild(modalContent);
}

/**
 * @returns {Promise<void>}
 */
async function updateFileList() {
  try {
    // Show loading state
    fileState.loading = true;
    fileState.error = null;

    // Update UI to show loading
    if (fileListDiv) {
      const loadingDiv = document.createElement("div");
      loadingDiv.id = "loading-indicator";
      loadingDiv.className = "flex justify-center items-center py-8";
      loadingDiv.innerHTML = `
        <div class='animate-spin rounded-full h-8 w-8 border-b-2 border-blue-500'></div>
        <span class='ml-3 text-gray-600'>Loading files...</span>
      `;
      fileListDiv.appendChild(loadingDiv);
    }

    const response = await fetch(`${CONFIG.API_URL}/files`, {
      headers: {
        Authorization: CONFIG.AUTH_TOKEN,
      },
    });

    if (!response.ok) {
      throw new Error(
        `Failed to fetch files: ${response.status} ${response.statusText}`,
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

    // Clear loading state
    fileState.loading = false;
    fileListDiv.innerHTML = "";

    // Update search results display in control section
    const searchResultsDisplay = document.getElementById(
      "search-results-display",
    );
    if (searchResultsDisplay) {
      if (fileState.searchTerm.trim()) {
        const resultText = filteredFiles.length === 1 ? "result" : "results";
        const hasTagFilters = allowedTags.size > 0 || deniedTags.size > 0;
        const filterText = hasTagFilters ? " (after applying tag filters)" : "";

        searchResultsDisplay.innerHTML = `
              <div class="p-3 bg-blue-50 border border-blue-200 rounded-lg">
                <div class="flex items-center justify-between">
                  <span class="text-sm text-blue-800">
                    <strong>${filteredFiles.length}</strong> ${resultText} found for "<em>${fileState.searchTerm}</em>"${filterText}
                  </span>
                  <button onclick="document.getElementById('file-search').focus()"
                          class="text-xs text-blue-600 hover:text-blue-800 underline">
                    Refine search
                  </button>
                </div>
              </div>
            `;
      } else {
        searchResultsDisplay.innerHTML = "";
      }
    }

    // Always add control section first
    const controlSection = createControlSection();
    fileListDiv.appendChild(controlSection);

    // Show empty state as a card if no files found
    if (filteredFiles.length === 0) {
      const emptyStateCard = document.createElement("div");
      emptyStateCard.className =
        "bg-white rounded-lg shadow-md p-6 text-center hover:shadow-lg transition-shadow duration-300";

      // Determine the type of empty state
      const hasTagFilters = allowedTags.size > 0 || deniedTags.size > 0;
      const hasSearchTerm = fileState.searchTerm.trim();

      let title,
        message,
        buttons = "";

      if (hasSearchTerm && hasTagFilters) {
        title = "No files found";
        message = `No files match both your search term '${fileState.searchTerm}' and your tag filters. Try adjusting either.`;
        buttons = `
          <button onclick="clearSearch()" class="px-4 py-2 bg-blue-500 text-white rounded hover:bg-blue-600 transition-colors mr-2">
            Clear Search
          </button>
          <button onclick="clearAll()" class="px-4 py-2 bg-gray-500 text-white rounded hover:bg-gray-600 transition-colors">
            Clear All Filters
          </button>
        `;
      } else if (hasSearchTerm) {
        title = "No files found";
        message = `Try adjusting your search term '${fileState.searchTerm}' or clear filters`;
        buttons = `
          <button onclick="clearSearch()" class="px-4 py-2 bg-blue-500 text-white rounded hover:bg-blue-600 transition-colors mr-2">
            Clear Search
          </button>
        `;
      } else if (hasTagFilters) {
        title = "No files match your filters";
        message =
          "No files have the selected tag combination. Try adjusting your tag filters";
        buttons = `
          <button onclick="clearAll()" class="px-4 py-2 bg-blue-500 text-white rounded hover:bg-blue-600 transition-colors">
            Clear Tag Filters
          </button>
        `;
      } else {
        title = "No files available";
        message = "Upload some files to get started";
      }

      emptyStateCard.innerHTML = `
        <div class='text-gray-400 text-6xl mb-4'>📁</div>
        <h3 class='text-lg font-medium text-gray-900 mb-2'>
          ${title}
        </h3>
        <p class='text-gray-600 mb-4'>
          ${message}
        </p>
        ${buttons}
      `;
      fileListDiv.appendChild(emptyStateCard);
      return;
    }

    filteredFiles.forEach((file) => {
      const fileItemDiv = document.createElement("div");
      fileItemDiv.className =
        "bg-white rounded-lg shadow-md p-6 hover:shadow-lg transition-shadow duration-300 cursor-pointer";
      fileItemDiv.onclick = () => showFileDetails(file);
      fileItemDiv.setAttribute("data-file-id", file.id);

      const { name, directory } = parseFilePath(file.path);

      const fileNameElement = document.createElement("p");
      fileNameElement.className = "text-xl font-semibold mb-2";

      // Highlight search terms in filename if searching
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

        fileNameElement.innerHTML = `${beforeMatch}<mark class='bg-yellow-200 px-1 rounded'>${match}</mark>${afterMatch}`;
      } else {
        fileNameElement.textContent = name;
      }

      const fileDirectoryElement = document.createElement("p");
      fileDirectoryElement.className = "text-gray-600 text-sm mb-4";
      fileDirectoryElement.textContent = directory;

      fileItemDiv.appendChild(fileNameElement);
      fileItemDiv.appendChild(fileDirectoryElement);

      // Add tags
      const tagsContainer = document.createElement("div");
      tagsContainer.className = "flex flex-wrap gap-2 tags";
      tagsContainer.setAttribute("data-file-id", file.id);

      file.tags.forEach((tag) => {
        const tagButton = document.createElement("button");
        tagButton.className = `px-3 py-1 rounded-full text-sm font-medium ${
          allowedTags.has(tag)
            ? "bg-green-500 text-white"
            : deniedTags.has(tag)
              ? "bg-red-500 text-white"
              : "bg-blue-500 text-white"
        } hover:bg-blue-600 transition-colors`;
        tagButton.textContent = tag;

        tagButton.onclick = (e) => {
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

        tagsContainer.appendChild(tagButton);
      });

      fileItemDiv.appendChild(tagsContainer);
      fileListDiv.appendChild(fileItemDiv);
    });
  } catch (error) {
    console.error("Error updating file list:", error);
    fileState.loading = false;
    fileState.error = error.message;

    // Show error state
    fileListDiv.innerHTML = "";
    const errorDiv = document.createElement("div");
    errorDiv.className = "col-span-full text-center py-12";
    errorDiv.innerHTML = `
      <div class='text-red-400 text-6xl mb-4'>⚠️</div>
      <h3 class='text-lg font-medium text-gray-900 mb-2'>
        Failed to load files
      </h3>
      <p class='text-gray-600 mb-4'>
        ${error.message}
      </p>
      <button onclick='updateFileList()'
              class='px-4 py-2 bg-blue-500 text-white rounded hover:bg-blue-600'>
        Try Again
      </button>
    `;
    fileListDiv.appendChild(errorDiv);
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

// === Initialization ===
getAllTags().then(() => updateFileList());
