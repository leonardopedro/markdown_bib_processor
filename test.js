import init, { process_markdown_and_bibtex } from './pkg/markdown_bib_processor.js';

// Test Cases
const testCases = [
    {
        name: "Basic Citation",
        markdown: "This is a test with @Test21.",
        bibtex: `@article{Test21, author="Test Author", title="Test Title", year=2021}`,
        prefix: "",
        expected_markdown: `This is a test with [[1]](#bibliography.md#Test21).`,
        expected_bib: `### Bibliography\n\n1.  <a name="Test21"></a>Test Author. (2021). *Test Title*.`
    },
    {
        name: "Multiple Citations",
        markdown: "Citing @One and @Two.",
        bibtex: `@misc{One, title="First"} \n@misc{Two, title="Second"}`,
        prefix: "refs/",
        expected_markdown: `Citing [[1]](refs/bibliography.md#One) and [[2]](refs/bibliography.md#Two).`,
        expected_bib: `### Bibliography\n\n1.  <a name="One"></a>*First*.\n2.  <a name="Two"></a>*Second*.`
    },
    {
        name: "Unknown Citation",
        markdown: "This should not be found @Unknown.",
        bibtex: `@misc{Known, title="Known Title"}`,
        prefix: "",
        expected_markdown: `This should not be found @Unknown.`,
        expected_bib: `### Bibliography`
    },
    {
        name: "Fuzzy Matching",
        markdown: "Testing fuzzy with @Smyth20.",
        bibtex: `@misc{Smith20, author="J. Smith", title="A Work"}`,
        prefix: "",
        expected_markdown: `Testing fuzzy with [[1]](#bibliography.md#Smith20).`,
        expected_bib: `### Bibliography\n\n1.  <a name="Smith20"></a>J. Smith. *A Work*.`
    },
    {
        name: "Empty Inputs",
        markdown: "",
        bibtex: "",
        prefix: "",
        expected_markdown: "",
        expected_bib: "### Bibliography"
    }
];

async function runTests() {
    try {
        await init();
        console.log("WASM Initialized for Testing");

        const resultsContainer = document.getElementById('test-results');

        for (const test of testCases) {
            const result = process_markdown_and_bibtex(test.markdown, test.bibtex, test.prefix);

            const md_pass = result.modified_markdown.trim() === test.expected_markdown.trim();
            const bib_pass = result.bibliography_markdown.trim() === test.expected_bib.trim();

            const testCaseElement = document.createElement('div');
            testCaseElement.classList.add('test-case');

            let content = `<h3>${test.name}</h3>`;
            content += `<div class="result ${md_pass ? 'pass' : 'fail'}">Markdown: ${md_pass ? 'PASS' : 'FAIL'}</div>`;
            if (!md_pass) {
                content += `<div>Expected: <pre>${test.expected_markdown}</pre></div>`;
                content += `<div>Got: <pre>${result.modified_markdown}</pre></div>`;
            }

            content += `<div class="result ${bib_pass ? 'pass' : 'fail'}">Bibliography: ${bib_pass ? 'PASS' : 'FAIL'}</div>`;
            if (!bib_pass) {
                content += `<div>Expected: <pre>${test.expected_bib}</pre></div>`;
                content += `<div>Got: <pre>${result.bibliography_markdown}</pre></div>`;
            }

            testCaseElement.innerHTML = content;
            resultsContainer.appendChild(testCaseElement);
        }

    } catch (err) {
        console.error("Error during testing:", err);
        const resultsContainer = document.getElementById('test-results');
        resultsContainer.innerHTML = `<div class="fail">FATAL: Could not initialize WASM module.</div>`;
    }
}

runTests();
