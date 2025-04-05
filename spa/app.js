const fileListDiv = document.getElementById('file-list');

// Add state management for current file
let currentFile = null;

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

        // Update the file list
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
        const fileListDiv = document.getElementById('file-list');
        fileListDiv.innerHTML = '';

        files.forEach(file => {
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
                const tagElement = document.createElement('span');
                tagElement.className = 'inline-block bg-blue-500 text-white px-3 py-1 rounded-full text-sm font-medium';
                tagElement.textContent = tag;
                tagsContainer.appendChild(tagElement);
            });

            fileItemDiv.appendChild(tagsContainer);
            fileListDiv.appendChild(fileItemDiv);
        });
    } catch (error) {
        console.error('Error updating file list:', error);
        alert('Failed to update file list');
    }
}

// Initial file list fetch
updateFileList();
