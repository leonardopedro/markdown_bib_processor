[<img src="https://deepwiki.com/badge.svg" alt="Ask DeepWiki" />](https://deepwiki.com/leonardopedro/markdown_bib_processor)

# Markdown Bibliography Processor

This is a command-line tool for processing Markdown files and BibTeX bibliographies. It corrects the Markdown (based on Streamdown.ai) which is useful for collaboration through loro.dev or with AI, it finds citation keys in the Markdown file, and appends a formatted bibliography at the end.

## How it works

The tool takes a Markdown file, a BibTeX file, a CSL (Citation Style Language) file, and a locale file as input. It parses the Markdown file to find citation keys in the format `[@LastnamefirstauthorLasttwodigitsofyearOptionalletterfromatoz]`. The last letter is optional (no letter same as 'a'), with the order determined by the year and then the alphabetic order of the titles corresponding to the same Lastnamefirstauthor (exact name without approximations) and Lasttwodigitsofyear.
The author's last name only needs to be approximately correct (useful for dealing with foreign characters)  It then uses the BibTeX file to find the corresponding bibliographic entries. Finally, it formats the bibliography according to the CSL style and appends it to the Markdown file.

## Usage

To use the tool, you need to provide the paths to the four input files:

```bash
cargo run -- --markdown <path/to/markdown.md> --bibtex <path/to/bib.bib> --csl <path/to/style.csl> --locale <path/to/locale.xml>
```

This will print the processed Markdown to standard output.

## Building from source

To build the project, you need to have the Rust toolchain installed. You can then build it using Cargo:

```bash
cargo build --release
```

The executable will be in `target/release/markdown_bib_processor`.

## WebAssembly (Wasm) Target

This project can also be compiled to a WebAssembly target.

To build for the `wasm32-wasip1` target, run:
```bash
cargo build --target=wasm32-wasip1
```

After building, you can run the Wasm module using a Wasm runtime like `wasmer`:
```bash
wasmer run markdown_bib_processor.wasm --mapdir /:. -- --markdown md.md     --bibtex bib.bib     --csl chicago.csl     --locale locales-en-US.xml
```

This makes the tool portable and capable of running in various environments that support WebAssembly.

## Dependencies

This project uses the following Rust crates:

- `clap`: For parsing command-line arguments.
- `hayagriva`: For formatting the bibliography.
- `nom-bibtex`: For parsing the BibTeX file.
- `regex`: For finding citation keys in the Markdown file.
- `levenshtein`: For finding the closest match for a citation key.
- `linked-hash-map`: To preserve the order of the bibliographic entries.
- `serde`: For serialization.
- `once_cell`: For one-time initialization of static values.
