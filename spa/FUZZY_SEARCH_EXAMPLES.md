# Fuzzy Search Examples and Usage Guide

This document provides examples and tips for using the fuzzy search functionality in the Archive Organizer SPA.

## How Fuzzy Search Works

The fuzzy search feature uses Fuse.js to provide intelligent filename matching. It searches both the filename and the full file path, allowing you to find files even with partial or approximate matches.

## Basic Usage Examples

### Exact Matches
```
Search: "document.pdf"
Matches: "document.pdf", "my_document.pdf", "important_document.pdf"
```

### Partial Matches
```
Search: "doc"
Matches: "document.pdf", "docs.txt", "medical_doc.docx", "documentation.md"
```

### Case Insensitive
```
Search: "IMG"
Matches: "IMG_001.jpg", "img_vacation.png", "image_file.gif"
```

### Year/Date Searches
```
Search: "2023"
Matches: "report_2023.pdf", "IMG_2023_01_15.jpg", "backup_2023_december.zip"
```

### File Extension Searches
```
Search: "pdf"
Matches: "document.pdf", "manual.pdf", "report_final.pdf"
```

## Advanced Search Patterns

### Multiple Word Fragments
```
Search: "img 2023"
Matches: "IMG_2023_vacation.jpg", "image_2023_family.png", "img_backup_2023.zip"
```

### Typos and Approximations
```
Search: "documnt" (missing 'e')
Matches: "document.pdf", "documents.txt", "my_document.docx"
```

### Underscore and Space Variations
```
Search: "my file"
Matches: "my_file.txt", "my-file.pdf", "myfile.doc"
```

## Search Configuration

The fuzzy search is configured with the following settings:

- **Threshold**: 0.3 (lower = more exact matches required)
- **Minimum Match Length**: 2 characters
- **Search Fields**: Both filename and full file path
- **Case Sensitivity**: Disabled (searches are case-insensitive)

## Keyboard Shortcuts

- **`/`** - Focus the search input (when not typing in another field)
- **`Escape`** - Clear the search and unfocus the input
- **`Enter`** - If only one result, open the file details

## Search Tips

### 1. Start with Key Terms
Begin with the most distinctive part of the filename:
```
✅ Good: "invoice" → finds "invoice_2023_jan.pdf"
❌ Less effective: "2023" → too many matches
```

### 2. Use Multiple Keywords
Combine different parts of the filename:
```
✅ Good: "report q4" → finds "quarterly_report_q4_2023.pdf"
✅ Good: "img vacation" → finds "IMG_vacation_2023.jpg"
```

### 3. Search by File Type
Include file extensions to narrow results:
```
✅ "pdf invoice" → finds invoice PDF files
✅ "jpg family" → finds family photos
```

### 4. Use Unique Identifiers
Search for unique numbers or codes:
```
✅ "INV001" → finds "invoice_INV001_march.pdf"
✅ "batch_47" → finds "data_batch_47_processed.csv"
```

## Combining with Tag Filters

Fuzzy search works seamlessly with tag filtering:

1. **Search First, Then Filter**: Type your search term, then click on tags to narrow results
2. **Filter First, Then Search**: Apply tag filters, then search within the filtered results
3. **Clear All**: Use the "Clear All" button to reset both search and tag filters

## Search Result Highlighting

When you search, matching portions of filenames are highlighted in yellow. This helps you quickly identify why a file matched your search term.

## Performance Tips

### For Large File Collections
- Use specific search terms to reduce the number of results
- Combine search with tag filters to narrow the scope
- The search is debounced (300ms delay) to avoid excessive API calls

### Search Responsiveness
- Results update in real-time as you type
- Use the clear button (✕) to quickly reset your search
- The search input remembers your last search term

## Common Search Patterns

### Finding Recent Files
```
Search: "2024" or "dec" or "latest"
```

### Finding Documents by Type
```
Search: "pdf report" or "docx proposal" or "xlsx budget"
```

### Finding Images
```
Search: "jpg" or "png" or "img" or "photo"
```

### Finding Archives
```
Search: "zip" or "backup" or "archive"
```

### Finding by Project
```
Search: "project alpha" or "client xyz" or "version 2"
```

## Troubleshooting

### No Results Found?
1. Check your spelling
2. Try shorter, more general terms
3. Clear tag filters that might be too restrictive
4. Use the "Clear All" button to reset everything

### Too Many Results?
1. Be more specific with your search terms
2. Add tag filters to narrow the results
3. Include file extensions in your search
4. Use multiple keywords to be more precise

### Search Not Working?
1. Make sure you have at least 2 characters in your search
2. Check that files are loaded (look for loading indicator)
3. Refresh the page if needed

## API Integration Notes

The search functionality works with the existing file API:
- Search is performed client-side for fast response
- File data is fetched once and cached for search performance
- Search results respect the same tag filtering logic as the main application