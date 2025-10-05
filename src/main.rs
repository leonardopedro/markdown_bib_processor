use std::fs;
use std::path::PathBuf;
use clap::Parser;

mod lib;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the input Markdown file
    #[arg(short, long)]
    markdown: PathBuf,

    /// Path to the input BibTeX file
    #[arg(short, long)]
    bibtex: PathBuf,

    /// Path to the output Markdown file for the modified content
    #[arg(short, long)]
    output: PathBuf,

    /// Path to the output Markdown file for the bibliography
    #[arg(short = 'f', long)]
    bib_output: PathBuf,

    /// Prefix for bibliography links in the modified Markdown
    #[arg(long, default_value = "bibliography.md")]
    link_prefix: String,
}

fn main() -> std::io::Result<()> {
    let args = Args::parse();

    // Read input files
    let markdown_input = fs::read_to_string(&args.markdown)?;
    let bibtex_input = fs::read_to_string(&args.bibtex)?;

    // Process the files
    match lib::process_markdown_and_bibtex(&markdown_input, &bibtex_input, &args.link_prefix) {
        Ok(output) => {
            // Write the modified markdown and bibliography to their respective files
            fs::write(&args.output, output.modified_markdown())?;
            fs::write(&args.bib_output, output.bibliography_markdown())?;

            println!("Successfully processed files.");
            println!("Modified Markdown written to: {:?}", &args.output);
            println!("Bibliography written to: {:?}", &args.bib_output);
        }
        Err(e) => {
            eprintln!("Error processing files: {}", e);
            // Convert the error to an io::Error to be returned
            return Err(std::io::Error::new(std::io::ErrorKind::Other, e));
        }
    }

    Ok(())
}
