#[cfg(test)]
mod tests {
    use super::*; // Imports process_markdown_and_bibtex, ProcessingOutput

    // Helper function to reduce boilerplate in tests
    fn run_test(
        markdown_input: &str,
        bibtex_input: &str,
        bibliography_link_prefix: &str,
        citation_style_name: &str,
        expected_markdown: &str,
        expected_bibliography_part_1: &str, // For multi-part bibliography checks
        expected_bibliography_part_2: Option<&str>, // For multi-part bibliography checks
    ) {
        match process_markdown_and_bibtex(
            markdown_input,
            bibtex_input,
            bibliography_link_prefix,
            citation_style_name,
        ) {
            Ok(output) => {
                assert_eq!(output.modified_markdown().trim(), expected_markdown.trim());

                // Normalize whitespace and newlines for bibliography comparison
                // Hayagriva can have subtle differences in newlines/spacing
                let normalize = |s: String| s.replace("\r\n", "\n").split_whitespace().collect::<Vec<_>>().join(" ");
                
                let actual_bib_normalized = normalize(output.bibliography_markdown());
                let expected_bib_part_1_normalized = normalize(expected_bibliography_part_1.to_string());

                assert!(actual_bib_normalized.contains(&expected_bib_part_1_normalized),
                    "Bibliography check (Part 1) failed.\nExpected to contain:\n{}\nActual:\n{}",
                    expected_bibliography_part_1_normalized, actual_bib_normalized);

                if let Some(part_2) = expected_bibliography_part_2 {
                    let expected_bib_part_2_normalized = normalize(part_2.to_string());
                    assert!(actual_bib_normalized.contains(&expected_bib_part_2_normalized),
                    "Bibliography check (Part 2) failed.\nExpected to contain:\n{}\nActual:\n{}",
                    expected_bib_part_2_normalized, actual_bib_normalized);
                }

                // Check overall structure: Starts with # Bibliography and has at least one ## <a name=
                assert!(output.bibliography_markdown().starts_with("# Bibliography"));
                if !expected_bibliography_part_1.is_empty() || (expected_bibliography_part_2.is_some() && !expected_bibliography_part_2.unwrap().is_empty()) {
                    assert!(output.bibliography_markdown().contains("## <a name="), "Bibliography missing anchor links");
                }


            }
            Err(js_val) => {
                // Convert JsValue to String for easier assertion, if possible.
                // This might be tricky as JsValue could be various JS types.
                // For now, we'll just panic if an error occurs where success is expected.
                panic!("process_markdown_and_bibtex failed: {:?}", js_val);
            }
        }
    }

    #[test]
    fn test_apa_style_and_anchors() {
        let markdown_input = "See @Smith20a and @Doe21.";
        let bibtex_input = r#"
@article{smith20first_key,
  author = {Smith, John and Collaborator, Jane},
  year = {2020},
  title = {First Great Paper},
  journal = {Journal of Studies},
  volume = {1},
  number = {1}, 
  pages = {1-10},
}
@book{doe2021_key,
  author = {Doe, Jane},
  year = {2021},
  title = {A Book on Everything},
  publisher = {Open Books},
  address = {New York},
}
        "#;
        let style = "apa";
        let link_prefix = "test_bib.html";
        let expected_markdown = "See [Smith20a](test_bib.html#smith20a) and [Doe21](test_bib.html#doe21).";
        
        // APA Order: Doe (2021) before Smith (2020) due to Hayagriva's default sorting for bibliography.
        // Also, APA style for journal articles is specific.
        // Example: Smith, J., & Collaborator, J. (2020). First Great Paper. *Journal of Studies*, *1*(1), 1–10.
        // Note: The number (issue) is often included if available.
        // The original expected output was:
        // ## <a name="doe21"></a>Doe, J. (2021). *A Book on Everything*. Open Books.
        // ## <a name="smith20a"></a>Smith, J., & Collaborator, J. (2020). First Great Paper. *Journal of Studies*, *1*(1), 1-10.
        // We'll check for key parts. Hayagriva might also add extra newlines or spacing.

        let expected_bib_doe = "## <a name=\"doe21\"></a>Doe, J. (2021). *A Book on Everything*. Open Books.";
        let expected_bib_smith = "## <a name=\"smith20a\"></a>Smith, J., & Collaborator, J. (2020). First Great Paper. *Journal of Studies*, *1*(1), 1–10.";


        run_test(
            markdown_input,
            bibtex_input,
            link_prefix,
            style,
            expected_markdown,
            expected_bib_doe, // Doe should come first in APA if sorted by author then year reverse
            Some(expected_bib_smith),
        );
    }

    #[test]
    fn test_mla_style_and_suffixes() {
        let markdown_input = "As shown by @BestAuth22a and @BestAuth22b.";
        let bibtex_input = r#"
@article{best_alpha_key,
  author = {Best, Author},
  year = {2022},
  title = {Alpha Work},
  journal = {Journal of Alpha},
}
@article{best_beta_key,
  author = {Best, Author},
  year = {2022},
  title = {Beta Work},
  journal = {Journal of Beta},
}
        "#;
        let style = "mla";
        let link_prefix = "mla_bib.html";
        let expected_markdown = "As shown by [BestAuth22a](mla_bib.html#bestauth22a) and [BestAuth22b](mla_bib.html#bestauth22b).";
        
        // MLA Order: Alpha before Beta due to title sorting for same author/year.
        // ## <a name="bestauth22a"></a>Best, Author. "Alpha Work." *Journal of Alpha*, 2022.
        // ## <a name="bestauth22b"></a>Best, Author. "Beta Work." *Journal of Beta*, 2022.
        let expected_bib_alpha = "## <a name=\"bestauth22a\"></a>Best, Author. \"Alpha Work.\" *Journal of Alpha*, 2022.";
        let expected_bib_beta = "## <a name=\"bestauth22b\"></a>Best, Author. \"Beta Work.\" *Journal of Beta*, 2022.";

        run_test(
            markdown_input,
            bibtex_input,
            link_prefix,
            style,
            expected_markdown,
            expected_bib_alpha,
            Some(expected_bib_beta),
        );
    }

    #[test]
    fn test_empty_input_apa() {
        run_test(
            "", // No markdown citations
            "", // No bibtex entries
            "prefix.html",
            "apa",
            "", // Expected empty markdown output
            "*(No citation keys found in Markdown input)*", // Expected bib message
            None,
        );
    }

    #[test]
    fn test_markdown_no_bibtex_match_apa() {
         run_test(
            "Cite @Unknown24.",
            "@article{somekey, author={Someone}, year={2023}, title={Title}}",
            "prefix.html",
            "apa",
            "Cite @Unknown24 [Reference Not Found].",
            "*(No BibTeX entries found matching any citation keys)*",
            None,
        );
    }
}
