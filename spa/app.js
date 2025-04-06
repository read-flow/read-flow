import pdfjsLib from 'pdfjs-dist';
import 'pdfjs-dist/web/pdf_viewer.css';

// Initialize PDF.js
pdfjsLib.GlobalWorkerOptions.workerSrc = `//cdnjs.cloudflare.com/ajax/libs/pdf.js/${pdfjsLib.version}/pdf.worker.min.js`;

// Create a global PDF viewer instance
let pdfViewer = null;
let currentPdfUrl = null;

// Function to initialize PDF viewer
async function initializePDFViewer(containerId, pdfUrl) {
    const container = document.getElementById(containerId);
    if (!container) {
        console.error('PDF container not found');
        return;
    }

    // Clean up existing viewer
    if (pdfViewer) {
        pdfViewer.destroy();
    }

    // Create new viewer
    pdfViewer = new pdfjsLib.PDFViewer({
        container,
        eventBus: new pdfjsLib.EventBus(),
        renderer: 'canvas',
        enableWebGL: true,
        enablePrintAutoRotate: true,
    });

    try {
        const loadingTask = pdfjsLib.getDocument(pdfUrl);
        const pdf = await loadingTask.promise;
        pdfViewer.setDocument(pdf);
    } catch (error) {
        console.error('Error loading PDF:', error);
    }
}

// Function to show PDF viewer
async function showPDFViewer(pdfUrl) {
    const pdfViewerContainer = document.getElementById('pdf-viewer-container');
    if (!pdfViewerContainer) return;

    // Only initialize if URL has changed
    if (pdfUrl !== currentPdfUrl) {
        await initializePDFViewer('pdf-viewer', pdfUrl);
        currentPdfUrl = pdfUrl;
    }

    pdfViewerContainer.classList.remove('hidden');
}

// Function to close PDF viewer
document.getElementById('close-pdf-viewer').addEventListener('click', () => {
    const pdfViewerContainer = document.getElementById('pdf-viewer-container');
    if (pdfViewerContainer) {
        pdfViewerContainer.classList.add('hidden');
    }
});

// Export the viewer functions
export { showPDFViewer };

// Initialize the application
async function init() {
    // Add event listener for file clicks
    document.getElementById('file-list').addEventListener('click', async (e) => {
        if (e.target.classList.contains('file-item')) {
            const fileData = JSON.parse(e.target.dataset.file);
            if (fileData.type === 'PDF') {
                await showPDFViewer(fileData.url);
            }
        }
    });
}

// Start the application
init();
