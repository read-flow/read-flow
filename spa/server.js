const express = require('express');
const cors = require('cors');
const path = require('path');

const app = express();

// Enable CORS
app.use(cors());

// Serve static files
app.use(express.static(path.join(__dirname, 'public')));

// API endpoints
app.get('/files', (req, res) => {
    // This should proxy to your actual API server
    res.json([]);
});

app.post('/files/:id/tags', (req, res) => {
    // This should proxy to your actual API server
    res.json({ success: true });
});

app.delete('/files/:id/tags', (req, res) => {
    // This should proxy to your actual API server
    res.json({ success: true });
});

const PORT = process.env.PORT || 3000;
app.listen(PORT, () => {
    console.log(`Server running on port ${PORT}`);
});
