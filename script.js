import init, { process_markdown_and_bibtex } from './pkg/markdown_bib_processor.js';

async function run() {
    try { // Add try/catch around init
        console.log("Attempting to initialize WASM...");
        await init();
        console.log("WASM Module Initialized SUCCESSFULLY");
    } catch (err) {
        console.error("FATAL: WASM Initialization Failed:", err);
        // Display error to the user if init fails
        const outputMarkdown = document.getElementById('output-markdown');
        const outputBibliography = document.getElementById('output-bibliography');
        if(outputMarkdown) outputMarkdown.textContent = "Error loading WASM module. Check console.";
        if(outputBibliography) outputBibliography.textContent = "Error loading WASM module. Check console.";
        return; // Stop execution if init fails
    }


    const processButton = document.getElementById('process-button');
    const markdownInput = document.getElementById('markdown-input');
    const bibtexInput = document.getElementById('bibtex-input');
    const outputMarkdown = document.getElementById('output-markdown');
    const outputBibliography = document.getElementById('output-bibliography');
    const bibLinkPrefixInput = document.getElementById('bib-link-prefix');

    // Check if elements were found
    if (!processButton) { console.error("Could not find button #process-button"); return; }
    if (!markdownInput) { console.error("Could not find textarea #markdown-input"); return; }
    if (!bibtexInput) { console.error("Could not find textarea #bibtex-input"); return; }
    if (!outputMarkdown) { console.error("Could not find pre #output-markdown"); return; }
    if (!outputBibliography) { console.error("Could not find pre #output-bibliography"); return; }
    if (!bibLinkPrefixInput) { console.error("Could not find input #bib-link-prefix"); return; }

    console.log("All elements found. Adding event listener...");

    processButton.addEventListener('click', () => {
        // --- Add log right at the start of the handler ---
        console.log("Process button CLICKED!");

        const mdText = markdownInput.value;
        const bibText = bibtexInput.value;
        const bibliographyLinkPrefix = bibLinkPrefixInput.value;
        console.log("Read inputs. Link prefix:", bibliographyLinkPrefix); // Log prefix value

        outputMarkdown.textContent = 'Processing...';
        outputBibliography.textContent = 'Processing...';
        console.log("Set outputs to 'Processing...'");

        try {
            console.log("Calling WASM function process_markdown_and_bibtex...");
            const result = process_markdown_and_bibtex(
                mdText,
                bibText,
                bibliographyLinkPrefix
            );
            console.log("WASM function returned."); // Check if it finishes

            outputMarkdown.textContent = result.modified_markdown;
            outputBibliography.textContent = result.bibliography_markdown;

            console.log("Processing complete. Outputs updated.");

        } catch (error) {
            console.error("Error during WASM call or processing:", error); // Log any error caught
            outputMarkdown.textContent = `Error: ${error}`;
            outputBibliography.textContent = `Error: ${error}`;
        }
    });

    console.log("Event listener attached successfully."); // Confirm listener setup
}

run(); // Execute the setup function
