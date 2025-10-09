# Archive Organizer - Single Page Application

A modern web application for organizing and managing archive files with tagging capabilities.

## Project Structure

```
spa/
├── public/                  # Static assets served by the web server
│   ├── app.js              # Main client-side application code
│   ├── index.html          # Main HTML file
│   └── styles.css          # Compiled CSS styles
├── server.js               # Express server for serving static files and API proxy
├── input.css               # Tailwind CSS configuration
├── package.json            # Project dependencies and scripts
├── package-lock.json       # Dependency lock file
├── tailwind.config.js      # Tailwind CSS configuration
└── README.md              # This file
```

## Features

- File listing and organization
- **Fuzzy search by filename** - Search files using smart fuzzy matching
- Tag-based filtering
- File details view
- Modern UI with Tailwind CSS
- Real-time file updates
- Tag management (add/remove)
- PDF file viewing with built-in PDF.js viewer

## Prerequisites

- Node.js (v18 or higher recommended)
- npm (comes with Node.js)

## Installation

1. Clone the repository
2. Navigate to the project directory:
   ```bash
   cd spa
   ```
3. Install dependencies:
   ```bash
   npm install
   ```

## Development

### Start Development Server

Run the development server with hot-reloading:
```bash
npm run dev
```

The application will be available at `http://localhost:3000`

### Build for Production

To build the CSS styles for production:
```bash
npm run build
```

## Configuration

The application is configured in `app.js` with the following settings:

- `API_URL`: Base URL for the backend API
- `AUTH_TOKEN`: Authentication token for API requests
- `PAGE_SIZE`: Number of files to display per page
- `TAG_COLORS`: Color configuration for different tag states

### Fuzzy Search Configuration

The fuzzy search feature is powered by Fuse.js with the following configuration:
- `threshold`: 0.3 (lower values = more exact matches)
- `minMatchCharLength`: 2 (minimum characters needed to trigger search)
- Search keys: filename and full file path
- Includes match highlighting and scoring

## Directory Structure

- `public/`: Contains all static assets that are served directly by the web server
  - `app.js`: Main client-side JavaScript application
  - `index.html`: Main HTML file
  - `styles.css`: Compiled CSS styles

- `server.js`: Express server that:
  - Serves static files from the public directory
  - Handles API requests (currently stubbed - needs backend integration)
  - Enables CORS for API requests

## PDF Viewing
The application includes a built-in PDF viewer using PDF.js that allows you to:
- View PDF files directly in the browser
- Zoom in/out of PDF documents
- Navigate through pages
- Print PDF documents
- Rotate pages automatically when printing

To view a PDF file:
1. Click on any PDF file in the archive list
2. The PDF viewer will open in a modal window
3. Use the close button in the top-right corner to exit the viewer

The PDF viewer is optimized for performance and includes features like:
- Hardware-accelerated rendering
- Progressive loading of PDF pages
- Support for large PDF documents
- Error handling for corrupted PDF files

## Usage

### Fuzzy Search
The application includes a powerful fuzzy search feature that allows you to quickly find files by name:

1. **Search Input**: Use the search box at the top of the file list to search for files
2. **Smart Matching**: The search uses fuzzy matching, so you don't need exact spelling:
   - "doc" will match "document.pdf", "my_doc.txt", etc.
   - "img2023" will match "image_2023.jpg", "IMG_2023.png", etc.
   - Partial matches work: "repo" matches "repository_backup.zip"
3. **Real-time Results**: Search results update as you type (with 300ms debouncing)
4. **Combined Filtering**: Search works alongside tag filtering - you can search and filter by tags simultaneously
5. **Clear Search**: Click the "✕" button in the search input to clear the search term

### Tag Filtering
- Click on any tag to add it to your filter
- Green tags: Show only files with ALL these tags
- Red tags: Hide files with ANY of these tags
- Gray tags: No filter applied
- Click "Clear All" to reset all tag filters

### File Management
- Click on any file to view detailed information
- Add/remove tags from the file details modal
- Download files directly from the detail view

## API Integration

The application is configured to work with a backend API. The current configuration in `app.js` points to `http://localhost:8000`. You'll need to:

1. Set up your backend server
2. Update the `API_URL` and `AUTH_TOKEN` in `app.js` to match your backend configuration
3. Implement the API endpoints in `server.js` to proxy requests to your backend

## Technologies Used

- Frontend:
  - JavaScript (ES6+)
  - Tailwind CSS
  - HTML5
  - PDF.js (for PDF viewing)
  - Fuse.js (for fuzzy search)

- Backend:
  - Node.js
  - Express
  - CORS

## License

ISC License - see LICENSE file for details
