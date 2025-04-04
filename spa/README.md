# Archive Organizer SPA

This directory contains the Single Page Application (SPA) frontend for the Archive Organizer project. The frontend is built using vanilla JavaScript and styled with Tailwind CSS.

## Project Structure

```
spa/
├── index.html         # Main HTML file
├── app.js            # JavaScript application logic
├── package.json      # Node.js project configuration
├── tailwind.config.js # Tailwind CSS configuration
└── README.md         # This file
```

## Prerequisites

- Node.js (v14 or higher)
- npm (comes with Node.js)

## Development Setup

1. Install dependencies:
   ```bash
   npm install
   ```

2. The project uses the Tailwind CSS CDN, so no additional build steps are required for development.

## Running the Development Server

Since this is a static frontend application, you can run it in several ways:

1. Using Python's built-in HTTP server (recommended for development):
   ```bash
   python3 -m http.server 5173
   ```
   Then open `http://localhost:5173` in your browser.

2. Using a simple Node.js server:
   ```bash
   npx serve
   ```

3. Or simply open the `index.html` file directly in your browser (though this won't work for API calls due to CORS restrictions).

## API Integration

The frontend communicates with the Archive Organizer backend server running at `http://localhost:8000`. All API requests require an authorization header:

```
Authorization: bearer secret
```

## Building for Production

1. The project uses Tailwind CSS CDN, so no build step is required.
2. Simply copy the contents of this directory to your production server.
3. Ensure the backend server is running and accessible.

## Testing

Currently, this project doesn't have automated tests. To test the application:

1. Start the backend server:
   ```bash
   cd ../archive_organizer
   cargo run --features server
   ```

2. Start the frontend development server (see "Running the Development Server" above).

3. Open the application in your browser and verify that:
   - Files are listed correctly
   - File names and directories are displayed properly
   - Tags are rendered as banners
   - Hover effects work on file cards

## Deployment

1. Copy the contents of this directory to your production server.
2. Ensure the backend server is running and accessible.
3. Configure your web server to serve the static files from this directory.

## Security

- All API requests require an authorization header.
- The frontend is served from a different origin than the backend, requiring proper CORS configuration.
- Keep the authorization token (`bearer secret`) secure in production.

## Contributing

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/AmazingFeature`)
3. Commit your changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

## License

This project is licensed under the MIT License - see the LICENSE file for details.
