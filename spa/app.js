const fileListDiv = document.getElementById('file-list');

fetch('http://localhost:8000/files', {
    headers: {
        'Authorization': 'bearer secret'
    }
})
    .then(response => response.json())
    .then(files => {
        files.forEach(file => {
            const fileItemDiv = document.createElement('div');
            fileItemDiv.className = 'bg-white rounded-lg shadow-md p-6 hover:shadow-lg transition-shadow duration-300';

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
            tagsContainer.className = 'flex flex-wrap gap-2';

            file.tags.forEach(tag => {
                const tagElement = document.createElement('span');
                tagElement.className = 'inline-block bg-blue-500 text-white px-3 py-1 rounded-full text-sm font-medium';
                tagElement.textContent = tag;
                tagsContainer.appendChild(tagElement);
            });

            fileItemDiv.appendChild(tagsContainer);
            fileListDiv.appendChild(fileItemDiv);
        });
    })
    .catch(error => console.error('Error fetching files:', error));
