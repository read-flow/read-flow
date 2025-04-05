const fileListDiv = document.getElementById('file-list');

// Add state management for current file
let currentFile = null;
let allowedTags = new Set();
let deniedTags = new Set();
let allTags = new Set();

// Function to get all unique tags from the files
async function getAllTags() {
    try {
        const response = await fetch('http://localhost:8000/files', {
            headers: {
                'Authorization': 'bearer secret'
            }
        });

        if (!response.ok) {
            throw new Error('Failed to fetch files');
        }

        const files = await response.json();
        files.forEach(file => {
            file.tags.forEach(tag => allTags.add(tag));
        });
    } catch (error) {
        console.error('Error fetching tags:', error);
    }
}

// Function to filter files by tags
function filterFilesByTags(files, allowedTags, deniedTags) {
    if (allowedTags.size === 0 && deniedTags.size === 0) return files;
    
    return files.filter(file => {
        const hasAllowedTags = allowedTags.size === 0 || 
            Array.from(allowedTags).every(tag => file.tags.includes(tag));
        const hasDeniedTags = deniedTags.size === 0 || 
            Array.from(deniedTags).every(tag => !file.tags.includes(tag));
        
        return hasAllowedTags && hasDeniedTags;
    });
}

// Function to create the control section
function createControlSection() {
    const controlSection = document.createElement('div');
    controlSection.className = 'bg-gray-50 p-4 mb-6 rounded-lg shadow-sm';

    // Create tag filters container and place it at the top
    const tagFiltersContainer = document.createElement('div');
    tagFiltersContainer.className = 'mb-4 flex flex-wrap gap-2';

    // Add buttons for each tag
    allTags.forEach(tag => {
        const button = document.createElement('button');
        button.className = `px-4 py-2 rounded-lg ${
            allowedTags.has(tag) ? 'bg-green-500 text-white' :
            deniedTags.has(tag) ? 'bg-red-500 text-white' :
            'bg-gray-200 text-gray-700'
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

    controlSection.appendChild(tagFiltersContainer);

    // Create selected tags container
    const selectedTagsContainer = document.createElement('div');
    selectedTagsContainer.className = 'mb-4';

    // Create allowed tags section
    const allowedSection = document.createElement('div');
    allowedSection.className = 'mb-4';
    
    const allowedLabel = document.createElement('div');
    allowedLabel.className = 'text-sm font-medium text-gray-700 mb-2';
    allowedLabel.textContent = 'Show files with all of these tags:';
    allowedSection.appendChild(allowedLabel);

    const allowedTagsContainer = document.createElement('div');
    allowedTagsContainer.className = 'flex flex-wrap gap-2';
    
    allowedTags.forEach(tag => {
        const tagButton = document.createElement('button');
        tagButton.className = `px-3 py-1 rounded-full text-sm font-medium ${
            allowedTags.has(tag) ? 'bg-green-500 text-white' :
            deniedTags.has(tag) ? 'bg-red-500 text-white' :
            'bg-blue-500 text-white'
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
        
        allowedTagsContainer.appendChild(tagButton);
    });
    
    allowedSection.appendChild(allowedTagsContainer);
    selectedTagsContainer.appendChild(allowedSection);

    // Create denied tags section
    const deniedSection = document.createElement('div');
    
    const deniedLabel = document.createElement('div');
    deniedLabel.className = 'text-sm font-medium text-gray-700 mb-2';
    deniedLabel.textContent = 'Hide files with any of these tags:';
    deniedSection.appendChild(deniedLabel);

    const deniedTagsContainer = document.createElement('div');
    deniedTagsContainer.className = 'flex flex-wrap gap-2';
    
    deniedTags.forEach(tag => {
        const tagButton = document.createElement('button');
        tagButton.className = `px-3 py-1 rounded-full text-sm font-medium ${
            allowedTags.has(tag) ? 'bg-green-500 text-white' :
            deniedTags.has(tag) ? 'bg-red-500 text-white' :
            'bg-blue-500 text-white'
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
        
        deniedTagsContainer.appendChild(tagButton);
    });
    
    deniedSection.appendChild(deniedTagsContainer);
    selectedTagsContainer.appendChild(deniedSection);

    // Add clear all button at the bottom
    const clearAllButton = document.createElement('button');
    clearAllButton.className = 'px-4 py-2 rounded-lg bg-gray-200 text-gray-700 hover:bg-gray-300 transition-colors';
    clearAllButton.textContent = 'Clear All';
    clearAllButton.onclick = () => {
        allowedTags.clear();
        deniedTags.clear();
        updateFileList();
    };
    selectedTagsContainer.appendChild(clearAllButton);

    controlSection.appendChild(selectedTagsContainer);
    return controlSection;
}

// Function to update the file list
async function updateFileList() {
    try {
        const response = await fetch('http://localhost:8000/files', {
            headers: {
                'Authorization': 'bearer secret'
            }
        });

        if (!response.ok) {
            throw new Error('Failed to fetch files');
        }

        const files = await response.json();
        const filteredFiles = filterFilesByTags(files, allowedTags, deniedTags);
        const fileListDiv = document.getElementById('file-list');
        fileListDiv.innerHTML = '';

        // Add control section
        const controlSection = createControlSection();
        fileListDiv.appendChild(controlSection);

        filteredFiles.forEach(file => {
            const fileItemDiv = document.createElement('div');
            fileItemDiv.className = 'bg-white rounded-lg shadow-md p-6 hover:shadow-lg transition-shadow duration-300 cursor-pointer';
            fileItemDiv.onclick = () => showFileDetails(file);
            fileItemDiv.setAttribute('data-file-id', file.id);

            const filePath = file.path;
            const fileName = filePath.split('/').pop();
            const fileDirectory = filePath.substring(0, filePath.lastIndexOf('/'));

            const fileNameElement = document.createElement('p');
            fileNameElement.className = 'text-xl font-semibold mb-2';
            fileNameElement.textContent = fileName;

            const fileDirectoryElement = document.createElement('p');
            fileDirectoryElement.className = 'text-gray-600 text-sm mb-4';
            fileDirectoryElement.textContent = fileDirectory;

            fileItemDiv.appendChild(fileNameElement);
            fileItemDiv.appendChild(fileDirectoryElement);

            // Add tags
            const tagsContainer = document.createElement('div');
            tagsContainer.className = 'flex flex-wrap gap-2 tags';
            tagsContainer.setAttribute('data-file-id', file.id);

            file.tags.forEach(tag => {
                const tagButton = document.createElement('button');
                tagButton.className = `px-3 py-1 rounded-full text-sm font-medium ${
                    allowedTags.has(tag) ? 'bg-green-500 text-white' :
                    deniedTags.has(tag) ? 'bg-red-500 text-white' :
                    'bg-blue-500 text-white'
                } hover:bg-blue-600 transition-colors`;
                tagButton.textContent = tag;
                
                // Add click handler to cycle through states
                tagButton.onclick = (e) => {
                    e.stopPropagation(); // Prevent file click handler from triggering
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
        console.error('Error updating file list:', error);
        alert('Failed to update file list');
    }
}

// Add function to display file details
function showFileDetails(file) {
    currentFile = file;

    // Create details container if it doesn't exist
    let detailsContainer = document.getElementById('file-details');
    if (!detailsContainer) {
        detailsContainer = document.createElement('div');
        detailsContainer.id = 'file-details';
        detailsContainer.className = 'fixed inset-0 bg-gray-900 bg-opacity-50 flex items-center justify-center z-50';
        document.body.appendChild(detailsContainer);
    }

    // Create modal content
    const modalContent = document.createElement('div');
    modalContent.className = 'bg-white rounded-lg p-8 w-full max-w-4xl mx-4 flex flex-col';

    // Add file information
    const fileName = file.path.split('/').pop();
    const fileDirectory = file.path.substring(0, file.path.lastIndexOf('/'));

    modalContent.innerHTML = `
        <h2 class="text-2xl font-bold mb-4">${fileName}</h2>
        <p class="text-gray-600 mb-6">${fileDirectory}</p>

        <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
            <!-- Left column -->
            <div>
                <h3 class="text-lg font-semibold mb-2">Basic Info</h3>
                <div class="space-y-4">
                    <div>
                        <span class="font-medium">Type:</span>
                        <span class="ml-2">${file.type_}</span>
                    </div>
                    <div>
                        <span class="font-medium">Size:</span>
                        <span class="ml-2">${(file.size / 1024).toFixed(2)} KB</span>
                    </div>
                    <div>
                        <span class="font-medium">Status:</span>
                        <span class="ml-2">${file.status}</span>
                    </div>
                    <div>
                        <span class="font-medium">ID:</span>
                        <span class="ml-2">#${file.id}</span>
                    </div>
                </div>
            </div>

            <!-- Right column -->
            <div>
                <h3 class="text-lg font-semibold mb-2">Tags</h3>
                <div class="flex flex-wrap gap-2">
                    ${file.tags.map(tag => `
                    <div class="relative">
                        <span class="inline-block bg-blue-500 text-white px-3 py-1 rounded-full text-sm font-medium">
                            ${tag}
                        </span>
                        <button 
                            class="absolute -right-1 -top-1 w-4 h-4 rounded-full bg-red-500 text-white hover:bg-red-600 transition-colors"
                            onclick="deleteTag(${file.id}, '${tag}')"
                            title="Remove tag"
                        >
                            <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"></path>
                            </svg>
                        </button>
                    </div>
                    `).join('')}
                </div>

                <h3 class="text-lg font-semibold mt-6 mb-2">Fingerprint</h3>
                <div class="bg-gray-50 p-3 rounded">
                    <code>${file.fingerprint}</code>
                </div>
            </div>
        </div>

        <div class="mt-8">
            <h3 class="text-lg font-semibold mb-2">File Path</h3>
            <div class="bg-gray-50 p-3 rounded">
                <code>${file.path}</code>
            </div>
        </div>

        <div class="mt-8">
            <h3 class="text-lg font-semibold mb-2">Description</h3>
            <p class="text-gray-600">${file.description || 'No description available'}</p>
        </div>

        <div class="mt-8 space-y-4">
            <h3 class="text-lg font-semibold mb-2">Actions</h3>
            <div class="space-y-2">
                <!-- Download Button -->
                <button
                    class="w-full bg-green-500 text-white px-4 py-2 rounded-lg hover:bg-green-600 transition-colors"
                    onclick="downloadFile(${file.id}, '${fileName}')"
                >
                    Download
                </button>
            </div>
        </div>

        <div class="mt-8 space-y-4">
            <h3 class="text-lg font-semibold mb-2">Tags</h3>
            <div class="flex flex-wrap gap-2" data-file-id="${file.id}">
                ${file.tags.map(tag => `
                <div class="relative">
                    <span class="inline-block bg-blue-500 text-white px-3 py-1 rounded-full text-sm font-medium">
                        ${tag}
                    </span>
                    <button 
                        class="absolute -right-1 -top-1 w-4 h-4 rounded-full bg-red-500 text-white hover:bg-red-600 transition-colors"
                        onclick="deleteTag(${file.id}, '${tag}')"
                        title="Remove tag"
                    >
                        <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"></path>
                        </svg>
                    </button>
                </div>
                `).join('')}
            </div>

            <!-- Add Tag Form -->
            <div class="mt-4">
                <div class="flex gap-2">
                    <input
                        type="text"
                        placeholder="Enter new tag"
                        class="flex-1 px-4 py-2 rounded-lg border border-gray-300 focus:outline-none focus:ring-2 focus:ring-blue-500"
                        id="tag-input-${file.id}"
                        onkeypress="handleTagKeyPress(event, ${file.id})"
                    >
                    <button
                        class="bg-blue-500 text-white px-4 py-2 rounded-lg hover:bg-blue-600 transition-colors"
                        onclick="addTag(${file.id})"
                    >
                        Add Tag
                    </button>
                </div>
            </div>
        </div>
    `;

    // Add close button at the bottom of the modal
    const closeButtonBottom = document.createElement('button');
    closeButtonBottom.className = 'mt-6 w-full bg-red-500 text-white px-6 py-2 rounded-lg hover:bg-red-600 transition-colors';
    closeButtonBottom.textContent = 'Close';
    closeButtonBottom.onclick = () => {
        detailsContainer.remove();
        currentFile = null;
    };

    // Add close button and modal content
    detailsContainer.innerHTML = '';
    detailsContainer.appendChild(modalContent);
    modalContent.appendChild(closeButtonBottom);

    // Add click outside handler to close modal
    detailsContainer.onclick = (e) => {
        if (e.target === detailsContainer) {
            detailsContainer.remove();
            currentFile = null;
        }
    };
}

// Add file operations functions
async function downloadFile(fileId, fileName) {
    try {
        const response = await fetch(`http://localhost:8000/files/${fileId}/download-as/${encodeURIComponent(fileName)}`, {
            headers: {
                'Authorization': 'bearer secret'
            }
        });

        if (!response.ok) {
            throw new Error('Failed to download file');
        }

        // Create a blob from the response and create a download link
        const blob = await response.blob();
        const url = window.URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = fileName;
        document.body.appendChild(a);
        a.click();
        window.URL.revokeObjectURL(url);
        document.body.removeChild(a);
    } catch (error) {
        console.error('Error downloading file:', error);
        alert('Failed to download file');
    }
}

// Tag management functions
function handleTagKeyPress(event, fileId) {
    if (event.key === 'Enter') {
        event.preventDefault();
        addTag(fileId);
    }
}

async function addTag(fileId) {
    const input = document.getElementById(`tag-input-${fileId}`);
    const newTag = input.value.trim();

    if (!newTag) {
        alert('Please enter a tag');
        return;
    }

    try {
        const response = await fetch(`http://localhost:8000/files/${fileId}/tags`, {
            method: 'POST',
            headers: {
                'Authorization': 'bearer secret',
                'Content-Type': 'application/json'
            },
            body: JSON.stringify([newTag])
        });

        if (!response.ok) {
            throw new Error('Failed to add tag');
        }

        // Get the updated file details
        const updatedFileResponse = await fetch(`http://localhost:8000/files/${fileId}`, {
            headers: {
                'Authorization': 'bearer secret'
            }
        });

        if (!updatedFileResponse.ok) {
            throw new Error('Failed to fetch updated file details');
        }

        const updatedFile = await updatedFileResponse.json();

        // Update the details card
        if (currentFile && currentFile.id === fileId) {
            currentFile = updatedFile;
            showFileDetails(updatedFile);
        }

        // Update the tag list
        allTags.add(newTag);
        updateFileList();

        // Clear the input
        input.value = '';
    } catch (error) {
        console.error('Error adding tag:', error);
        alert('Failed to add tag');
    }
}

async function deleteTag(fileId, tag) {
    try {
        const response = await fetch(`http://localhost:8000/files/${fileId}/tags`, {
            method: 'DELETE',
            headers: {
                'Authorization': 'bearer secret',
                'Content-Type': 'application/json'
            },
            body: JSON.stringify([tag])
        });

        if (!response.ok) {
            throw new Error('Failed to delete tag');
        }

        // Get the updated file details
        const updatedFileResponse = await fetch(`http://localhost:8000/files/${fileId}`, {
            headers: {
                'Authorization': 'bearer secret'
            }
        });

        if (!updatedFileResponse.ok) {
            throw new Error('Failed to fetch updated file details');
        }

        const updatedFile = await updatedFileResponse.json();

        // Update the details card
        if (currentFile && currentFile.id === fileId) {
            currentFile = updatedFile;
            showFileDetails(updatedFile);
        }

        // Update the file list
        updateFileList();
    } catch (error) {
        console.error('Error deleting tag:', error);
        alert('Failed to delete tag');
    }
}

// Initial file list fetch
getAllTags().then(() => updateFileList());
