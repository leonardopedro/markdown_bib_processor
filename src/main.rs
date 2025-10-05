// cargo build --target=wasm32-wasip1
// wasmer run markdown_bib_processor.wasm --mapdir /:. -- --markdown md.md     --bibtex bib.bib     --csl chicago.csl     --locale locales-en-US.xml

use clap::Parser;
use std::fs;
use std::path::PathBuf;

// Import the function from the library crate
use markdown_bib_processor::process_markdown_and_bibtex;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the input Markdown file
    #[arg(long)]
    markdown: PathBuf,

    /// Path to the input BibTeX file
    #[arg(long)]
    bibtex: PathBuf,

    /// Path to the CSL style file (e.g., chicago-author-date.csl)
    #[arg(long)]
    csl: PathBuf,

    /// Path to the CSL locale file (e.g., en-US.xml)
    #[arg(long)]
    locale: PathBuf,
}

fn main() -> std::io::Result<()> {
    let args = Args::parse();

    // Read the content from the files specified in the command-line arguments
    let markdown_input = fs::read_to_string(args.markdown)?;
    let bibtex_input = fs::read_to_string(args.bibtex)?;
    let csl_input = fs::read_to_string(args.csl)?;
    let locale_input = fs::read_to_string(args.locale)?;

    // Call the library function to process the inputs
    match process_markdown_and_bibtex(
        &markdown_input,
        &bibtex_input,
        "", // Using an empty string for the link prefix
        &csl_input,
        &locale_input,
    ) {
        Ok(output) => {
            // Combine the processed markdown and the bibliography and print to console
            let final_document = format!(
                "{}\n\n{}",
                output.modified_markdown, output.bibliography_markdown
            );
            println!("{}", final_document);
        }
        Err(e) => {
            eprintln!("Error processing files: {}", e);
            // Return an I/O error to terminate the process
            return Err(std::io::Error::new(std::io::ErrorKind::Other, e));
        }
    }

    Ok(())
}
