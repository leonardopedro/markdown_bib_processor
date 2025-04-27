// Import the WASM functions and types
import init, { process_markdown_and_bibtex } from './pkg/markdown_bib_processor.js';

async function run() {
    // Initialize the WASM module
    await init();
    console.log("WASM Module Initialized");

    const processButton = document.getElementById('process-button');
    const markdownInput = document.getElementById('markdown-input');
    const bibtexInput = document.getElementById('bibtex-input');
    const outputMarkdown = document.getElementById('output-markdown');
    const outputBibliography = document.getElementById('output-bibliography');
    // MODIFICATION 2: Get the filename input element
    const bibFilenameInput = document.getElementById('bib-filename');
    // End Modification 2

    processButton.addEventListener('click', () => {
        const mdText = markdownInput.value;
        const bibText = bibtexInput.value;

        // MODIFICATION 2: Read filename, use default if empty
        let bibliographyFilename = bibFilenameInput.value.trim();
        if (bibliographyFilename === "") {
            bibliographyFilename = "bibliography.md"; // Default value
        }
        // End Modification 2

        outputMarkdown.textContent = 'Processing...';
        outputBibliography.textContent = 'Processing...';

        try {
            // MODIFICATION 2: Pass the potentially custom filename
            const result = process_markdown_and_bibtex(
                mdText,
                bibText,
                bibliographyFilename // Pass the actual filename to use
            );

            outputMarkdown.textContent = result.modified_markdown;
            outputBibliography.textContent = result.bibliography_markdown;

            console.log("Processing complete.");

        } catch (error) {
            console.error("Error processing citations:", error);
            outputMarkdown.textContent = `Error: ${error}`;
            outputBibliography.textContent = `Error: ${error}`;
        }
    });

     console.log("Event listener attached.");

}

run();
