#[cfg(test)]
mod tests;

use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;
use regex::{Regex, Captures};
use std::collections::{HashMap, HashSet};
use levenshtein::levenshtein; // For fuzzy matching

// --- Hayagriva Imports ---
use hayagriva::io::from_biblatex_str;
use hayagriva::style::{
    BibliographyDriver, BibliographyRequest, CitationItem, CitationRequest,
    ArchivedStyle, // For bundled CSL styles
    Locale,      // For bundled locales
    LocaleFile, // For loading locale XML
};
use hayagriva::Entry;
use std::io::Write; // For writing formatted output (not directly used here, but good practice for Hayagriva)


#[cfg(feature = "console_error_panic_hook")]
extern crate console_error_panic_hook;

macro_rules! log {
    ( $( $t:tt )* ) => {
        #[cfg(target_arch = "wasm32")]
        web_sys::console::log_1(&format!( $( $t )* ).into());
        #[cfg(not(target_arch = "wasm32"))]
        println!( $( $t )* );
    }
}

// --- Constants ---
const FUZZY_MATCH_THRESHOLD: usize = 10; // Max Levenshtein distance for fuzzy author matching

// --- Helper Functions (Adapted for Hayagriva) ---

fn get_first_author_last_name_hayagriva(entry: &Entry) -> Option<String> {
    entry.authors().get(0).and_then(|agent| {
        agent.surname.as_ref().map(|s| s.to_lowercase())
        // Fallback if surname is not present, try to get last part of name
        .or_else(|| agent.name.split_whitespace().last().map(|s| s.to_lowercase()))
    })
}

fn get_year_yy_hayagriva(entry: &Entry) -> Option<String> {
    entry.date().and_then(|date_val| {
        let year_str = match date_val {
            hayagriva::DateValue::Point(p) => p.year.to_string(),
            hayagriva::DateValue::Range(r) => r.start.year.to_string(), // Or end, or handle more gracefully
        };
        if year_str.len() >= 2 {
            Some(year_str.chars().skip(year_str.len() - 2).collect())
        } else { None }
    })
}

fn suffix_to_index(suffix: &str) -> usize {
    if suffix.is_empty() { 0 }
    else { (suffix.chars().next().unwrap_or('a') as u32).saturating_sub('a' as u32) as usize }
}

fn get_entry_title_for_sort_hayagriva(entry: &Entry) -> String {
    entry.title()
        .map(|t| t.value.to_lowercase()) // Assuming Text::value is the main string
        .unwrap_or_else(|| "".to_string())
}

// Anchor creation remains the same (suppresses 'a')
fn create_anchor(author_part: &str, year_part: &str, suffix_part: &str) -> String {
    let base = format!("{}{}", author_part, year_part).to_lowercase();
    if suffix_part.is_empty() || suffix_part == "a" { base }
    else { format!("{}{}", base, suffix_part) }
}

// --- Main WASM Function ---

#[wasm_bindgen]
#[derive(serde::Serialize, Debug)]
pub struct ProcessingOutput {
    modified_markdown: String,
    bibliography_markdown: String,
}

#[wasm_bindgen]
impl ProcessingOutput {
    #[wasm_bindgen(getter)] pub fn modified_markdown(&self) -> String { self.modified_markdown.clone() }
    #[wasm_bindgen(getter)] pub fn bibliography_markdown(&self) -> String { self.bibliography_markdown.clone() }
}

#[wasm_bindgen]
pub fn process_markdown_and_bibtex(
    markdown_input: &str,
    bibtex_input: &str,
    bibliography_link_prefix: &str,
    citation_style_name: &str, // New parameter
) -> Result<ProcessingOutput, JsValue> {

    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();

    // --- 1. Define Regex & Find Unique Citations from Markdown ---
    let citation_regex = Regex::new(r"(@([a-zA-Z]+)(\d{2})([a-z]?))\b")
        .map_err(|e| JsValue::from_str(&format!("Regex error: {}", e)))?;

    let mut unique_citations: HashMap<String, (String, String, String)> = HashMap::new();
    for cap in citation_regex.captures_iter(markdown_input) {
        let full_match = cap.get(1).map_or("", |m| m.as_str()).to_string();
        let author_part = cap.get(2).map_or("", |m| m.as_str()).to_string();
        let year_part = cap.get(3).map_or("", |m| m.as_str()).to_string();
        let suffix_part = cap.get(4).map_or("", |m| m.as_str()).to_string();
        if !full_match.is_empty() {
             unique_citations.entry(full_match).or_insert((author_part, year_part, suffix_part));
        }
    }
    log!("Found {} unique citation keys in Markdown.", unique_citations.len());

    // --- 2. Parse BibTeX using Hayagriva ---
    let bib_db = from_biblatex_str(bibtex_input) // bib_db is IndexMap<String, Entry>
        .map_err(|e| JsValue::from_str(&format!("Hayagriva BibTeX parsing error: {:?}", e)))?;
    log!("Parsed {} BibTeX entries using Hayagriva.", bib_db.len());

    // --- 3. Map Markdown keys to specific Hayagriva Entries ---
    // final_entry_map: Markdown Key -> Hayagriva Entry Reference
    let mut final_entry_map: HashMap<String, &Entry> = HashMap::new();
    let mut missing_keys: HashSet<String> = unique_citations.keys().cloned().collect();

    for (md_key, (author_part, year_part, suffix_part)) in &unique_citations {
        let md_author_lc = author_part.to_lowercase();
        let mut candidate_entries: Vec<&Entry> = Vec::new();

        // --- 3a. Collect Candidates (Fuzzy Author, Exact Year) ---
        for entry in bib_db.values() {
            if let Some(entry_year_yy) = get_year_yy_hayagriva(entry) {
                if entry_year_yy == *year_part { // Year matches
                    if let Some(entry_author_lastname_lc) = get_first_author_last_name_hayagriva(entry) {
                        // Try exact author match first
                        if entry_author_lastname_lc == md_author_lc {
                            candidate_entries.push(entry);
                            continue; // Prioritize exact matches
                        }
                        // Then try fuzzy author match
                        let distance = levenshtein(&md_author_lc, &entry_author_lastname_lc);
                        if distance <= FUZZY_MATCH_THRESHOLD {
                            candidate_entries.push(entry);
                        }
                    }
                }
            }
        }

        // --- 3b. Sort Candidates by Title ---
        candidate_entries.sort_by_key(|e| get_entry_title_for_sort_hayagriva(e));

        // --- 3c. Select Entry using Suffix ---
        let index = suffix_to_index(suffix_part);
        if let Some(selected_entry) = candidate_entries.get(index) {
            final_entry_map.insert(md_key.clone(), selected_entry);
            missing_keys.remove(md_key);
            log!("Mapped key '{}' to BibTeX entry '{}' (Candidates found: {}, Index: {}).", md_key, selected_entry.key(), candidate_entries.len(), index);
        } else {
            if !candidate_entries.is_empty() {
                 log!("Warning: Suffix '{}' for key '{}' is out of bounds (candidates: {}, index: {}).", suffix_part, md_key, candidate_entries.len(), index);
            } else {
                 log!("Warning: No candidates found for key '{}' (Author: {}, Year: {}).", md_key, author_part, year_part);
            }
        }
    } // End loop through unique_citations

    // Log all keys that couldn't be mapped
    for missing_key in &missing_keys {
        log!("Warning: Could not map key '{}' to any BibTeX entry.", missing_key);
    }

    // --- 4. Prepare for Bibliography Generation ---

    // Sort unique markdown citation keys: primarily by author/year, then by suffix.
    // This helps in selecting a "primary" markdown key if multiple point to the same BibTeX entry.
    let mut sorted_unique_citations_keys: Vec<String> = unique_citations.keys().cloned().collect();
    sorted_unique_citations_keys.sort_by(|a, b| {
        let (author_a, year_a, suffix_a) = unique_citations.get(a).unwrap();
        let (author_b, year_b, suffix_b) = unique_citations.get(b).unwrap();
        author_a.to_lowercase().cmp(&author_b.to_lowercase())
            .then_with(|| year_a.cmp(year_b))
            .then_with(|| suffix_to_index(suffix_a).cmp(&suffix_to_index(suffix_b)))
    });

    // Create a list of (Markdown Key, &Entry) for entries that will be in the bibliography.
    // Deduplicate based on the BibTeX entry's own key.
    // The Markdown key retained is the first one encountered according to `sorted_unique_citations_keys`.
    let mut bibliography_items_to_render_hayagriva : Vec<(&String, &Entry)> = Vec::new();
    let mut rendered_bib_keys_hayagriva = HashSet::new(); // Tracks BibTeX citation keys (entry.key())
    for md_key in &sorted_unique_citations_keys {
        if let Some(entry_ref) = final_entry_map.get(md_key) { // entry_ref is &&Entry
             if rendered_bib_keys_hayagriva.insert((*entry_ref).key().to_string()) {
                 bibliography_items_to_render_hayagriva.push((md_key, *entry_ref)); // md_key is &String, *entry_ref is &Entry
             }
        }
    }

    // Sort this final list for bibliography output (Author, Year, Title)
    bibliography_items_to_render_hayagriva.sort_by(|(_, entry_a), (_, entry_b)|{
         let author_a = get_first_author_last_name_hayagriva(entry_a).unwrap_or_default();
         let author_b = get_first_author_last_name_hayagriva(entry_b).unwrap_or_default();
         let year_a = get_year_yy_hayagriva(entry_a).unwrap_or_default();
         let year_b = get_year_yy_hayagriva(entry_b).unwrap_or_default();
         let title_a = get_entry_title_for_sort_hayagriva(entry_a);
         let title_b = get_entry_title_for_sort_hayagriva(entry_b);
         author_a.cmp(&author_b)
            .then_with(|| year_a.cmp(&year_b))
            .then_with(|| title_a.cmp(&title_b))
    });
    log!("Prepared {} unique BibTeX entries for the bibliography section.", bibliography_items_to_render_hayagriva.len());


    // --- 5. Generate Bibliography using Hayagriva ---
    let mut bibliography_markdown_lines: Vec<String> = Vec::new();
    bibliography_markdown_lines.push("# Bibliography".to_string());
    bibliography_markdown_lines.push("".to_string());

    if !bibliography_items_to_render_hayagriva.is_empty() {
        let style = ArchivedStyle::by_name(citation_style_name)
            .ok_or_else(|| JsValue::from_str(&format!("Citation style '{}' not found in Hayagriva's archive.", citation_style_name)))?;

        let locale_name = "en-US"; // Default locale
        let locale_xml = hayagriva::archive::LOCALES.get(locale_name)
           .ok_or_else(|| JsValue::from_str(&format!("Locale '{}' not found in Hayagriva's archive.", locale_name)))?;
        let locale_file = LocaleFile::from_xml(locale_xml)
           .map_err(|e| JsValue::from_str(&format!("Failed to parse locale '{}': {:?}", locale_name, e)))?;
        let locales_for_driver = [locale_file.into()]; // Hayagriva expects an array of Locale

        let mut driver = BibliographyDriver::new();

        // Inform the driver about all entries that will appear in the bibliography.
        // This allows it to handle numbering, disambiguation, etc., according to the style.
        // We pass them in the order they should appear in the bibliography.
        for (_, entry) in &bibliography_items_to_render_hayagriva {
            let item = CitationItem::from_entry(*entry);
            // The driver.citation() call is typically for in-text citations to get their formatting.
            // However, it also registers the entry with the driver for context.
            // For bibliography-only generation, we still need to "mention" items to the driver.
            driver.citation(CitationRequest::from_items(std::iter::once(item), &style, &locales_for_driver));
        }

        let bib_request = BibliographyRequest {
           style: &style,
           // locale: Some(&locales_for_driver[0]), // Pass the specific locale instance
           locale_files: &locales_for_driver, // Pass the array of LocaleFile derived objects
        };
        let formatted_bib = driver.finish(bib_request); // This consumes the driver

        // The `formatted_bib.entries` are Vec<FormattedBibliographyEntry>
        // These should correspond to the entries in `bibliography_items_to_render_hayagriva`
        // IF Hayagriva preserves the order of citation registration for bibliography output.
        // Let's iterate `formatted_bib.entries` and map back to our `md_key` for anchors.

        if formatted_bib.entries.len() != bibliography_items_to_render_hayagriva.len() {
            log!("Warning: Mismatch in length between Hayagriva output ({}) and expected items ({}). Anchors might be unreliable.",
                 formatted_bib.entries.len(), bibliography_items_to_render_hayagriva.len());
        }

        for fmt_entry in formatted_bib.entries.iter() {
            // `fmt_entry.entry` is an `Rc<Entry>`. We need its key to find the original md_key.
            let hayagriva_entry_key = fmt_entry.entry.key();

            // Find the (md_key, &Entry) from our sorted list that corresponds to this formatted entry.
            // This md_key is used for generating the anchor.
            let (md_key_for_anchor, _entry_for_anchor) = bibliography_items_to_render_hayagriva.iter()
                .find(|(_, e)| e.key() == hayagriva_entry_key)
                .ok_or_else(|| JsValue::from_str(&format!("Formatted entry for BibTeX key '{}' not found in our internal render list. Cannot generate anchor.", hayagriva_entry_key)))?;

            let (author_part, year_part, suffix_part) = unique_citations.get(*md_key_for_anchor)
                .ok_or_else(|| JsValue::from_str("Markdown key components not found for anchor generation."))?; // Should not happen

            let anchor = create_anchor(author_part, year_part, suffix_part);

            let entry_text_str = String::from_utf8(fmt_entry.text.clone())
                .map_err(|e| JsValue::from_str(&format!("UTF-8 conversion error for bib entry text: {}", e)))?;

            // Hayagriva's output often includes a final newline, trim it for cleaner display.
            let heading = format!("## <a name=\"{}\"></a>{}", anchor, entry_text_str.trim_end_matches('\n'));
            bibliography_markdown_lines.push(heading);
            bibliography_markdown_lines.push("".to_string());
        }
        let heading = format!("### <a name=\"{}\"></a>{}", anchor, formatted_entry);
        bibliography_markdown_lines.push(heading);
        bibliography_markdown_lines.push("".to_string());
    }

    } else { // bibliography_items_to_render_hayagriva is empty
        if !missing_keys.is_empty() {
            bibliography_markdown_lines.push("*(No BibTeX entries found matching any citation keys)*".to_string());
        } else if unique_citations.is_empty() {
             bibliography_markdown_lines.push("*(No citation keys found in Markdown input)*".to_string());
        } else {
            // This case implies all unique_citations were filtered out before rendering,
            // e.g. all keys were invalid or pointed to non-existent entries.
            bibliography_markdown_lines.push("*(No valid BibTeX entries to display based on Markdown citations)*".to_string());
        }
    }
    let bibliography_content = bibliography_markdown_lines.join("\n");


    // --- 6. Replace citations in Markdown with Links ---
    let modified_markdown_content = citation_regex.replace_all(markdown_input, |caps: &Captures| {
        let full_match = caps.get(1).map_or("", |m| m.as_str());

        if final_entry_map.contains_key(full_match) { // Check if we successfully mapped this md_key
             let author_part = caps.get(2).map_or("", |m| m.as_str());
             let year_part = caps.get(3).map_or("", |m| m.as_str());
             let suffix_part = caps.get(4).map_or("", |m| m.as_str());

             let anchor = create_anchor(author_part, year_part, suffix_part);

             let link_text = if suffix_part.is_empty() || suffix_part == "a" {
                 format!("{}{}", author_part, year_part) // Suppress 'a' for display
             } else {
                 format!("{}{}{}", author_part, year_part, suffix_part)
             };
             format!("[{}]({}#{})", link_text, bibliography_link_prefix, anchor)
        } else {
            // If the key was in unique_citations but not in final_entry_map, it means it wasn't found/matched.
            log!("Markdown Replacement: Key '{}' was not found in final_entry_map. Marking as 'Reference Not Found'.", full_match);
            format!("{} [Reference Not Found]", full_match)
        }
    }).to_string();

    log!("Markdown processing complete.");

    Ok(ProcessingOutput {
        modified_markdown: modified_markdown_content,
        bibliography_markdown: bibliography_content,
    })
}
