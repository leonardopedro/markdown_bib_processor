      
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
    const bibliographyFilename = "bibliography.md"; // Consistent filename for links

    processButton.addEventListener('click', () => {
        const mdText = markdownInput.value;
        const bibText = bibtexInput.value;

        outputMarkdown.textContent = 'Processing...';
        outputBibliography.textContent = 'Processing...';

        try {
            // Call the Rust WASM function
            const result = process_markdown_and_bibtex(mdText, bibText, bibliographyFilename);

            // Access results using generated getters
            outputMarkdown.textContent = result.modified_markdown;
            outputBibliography.textContent = result.bibliography_markdown;

            console.log("Processing complete.");
            // You might want to free the memory if the result object isn't automatically handled
            // by JS garbage collection or if you were returning raw pointers (not needed here).
            // result.free(); // Only if the struct had a #[wasm_bindgen] destructor

        } catch (error) {
            console.error("Error processing citations:", error);
            outputMarkdown.textContent = `Error: ${error}`;
            outputBibliography.textContent = `Error: ${error}`;
        }
    });

     console.log("Event listener attached.");
     // Initial processing on load (optional)
     // processButton.click();

}

run();

    

IGNORE_WHEN_COPYING_START

