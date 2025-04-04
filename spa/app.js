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
            fileItemDiv.classList.add('file-item');

            const filePath = file.path;
            const fileName = filePath.split('/').pop();
            const fileDirectory = filePath.substring(0, filePath.lastIndexOf('/'));

            const fileNameElement = document.createElement('p');
            fileNameElement.classList.add('file-name');
            fileNameElement.textContent = fileName;
            const fileDirectoryElement = document.createElement('p');
            fileDirectoryElement.classList.add('file-directory');
            fileDirectoryElement.textContent = fileDirectory;

            fileItemDiv.appendChild(fileNameElement);
            fileItemDiv.appendChild(fileDirectoryElement);

            // Add tags
            file.tags.forEach(tag => {
                const tagElement = document.createElement('span');
                tagElement.classList.add('tag');
                tagElement.textContent = tag;
                fileItemDiv.appendChild(tagElement);
            });

            fileListDiv.appendChild(fileItemDiv);
        });
    })
    .catch(error => console.error('Error fetching files:', error));
