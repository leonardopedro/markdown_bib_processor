use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;
use regex::{Regex, Captures};
use nom_bibtex::{Bibtex, Bibliography};
use std::collections::{HashMap, HashSet};
// For fuzzy matching
use levenshtein::levenshtein;


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

// --- Helper Functions ---

fn get_first_author_last_name(author_string: &str) -> Option<String> {
    let first_author_part = author_string.split(" and ").next()?.trim();
    if let Some((last, _)) = first_author_part.split_once(',') {
        return Some(last.trim().to_lowercase());
    }
    first_author_part.split_whitespace().last().map(|s| s.trim().to_lowercase())
}

fn get_year_yy(year_string: &str) -> Option<String> {
    let trimmed_year = year_string.trim();
    if trimmed_year.len() >= 2 {
        Some(trimmed_year.chars().skip(trimmed_year.len() - 2).collect())
    } else { None }
}

fn suffix_to_index(suffix: &str) -> usize {
    if suffix.is_empty() { 0 }
    else { (suffix.chars().next().unwrap_or('a') as u32).saturating_sub('a' as u32) as usize }
}

fn get_entry_title_for_sort(entry: &Bibliography) -> String {
    entry.tags().iter()
        .find(|(k, _v)| k.eq_ignore_ascii_case("title"))
        .map(|(_k, v)| v.trim_matches(|c| c == '{' || c == '}' || c == '"').to_lowercase())
        .unwrap_or_else(|| "".to_string())
}

// Anchor creation remains the same (suppresses 'a')
fn create_anchor(author_part: &str, year_part: &str, suffix_part: &str) -> String {
    let base = format!("{}{}", author_part, year_part).to_lowercase();
    if suffix_part.is_empty() || suffix_part == "a" { base }
    else { format!("{}{}", base, suffix_part) }
}

// format_bib_entry_for_markdown remains the same
fn format_bib_entry_for_markdown(entry: &Bibliography) -> String {
    let mut parts: Vec<String> = Vec::new();
    let tags = entry.tags();
    let find_tag = |key: &str| -> Option<String> { tags.iter().find(|(k, _v)| k.eq_ignore_ascii_case(key)).map(|(_k, v)| v.clone()) };
    if let Some(author) = find_tag("author") { parts.push(author.replace(" and ", ", ")); } else { parts.push("Unknown Author".to_string()); }
    if let Some(year) = find_tag("year") { parts.push(format!("({})", year)); } else { parts.push("(N.D.)".to_string()); }
    if let Some(title) = find_tag("title") { let clean_title = title.trim_matches(|c| c == '{' || c == '}' || c == '"'); parts.push(format!("*{}.*", clean_title)); } else { parts.push("*No Title*.".to_string()); }
    let mut source = String::new();
    if let Some(journal) = find_tag("journal") { source.push_str(&format!(" *{}*", journal.trim_matches(|c| c == '{' || c == '}' || c == '"'))); if let Some(volume) = find_tag("volume") { source.push_str(&format!(", {}", volume)); } if let Some(pages) = find_tag("pages") { source.push_str(&format!(", pp. {}", pages.replace("--", "-"))); } source.push('.'); } else if let Some(booktitle) = find_tag("booktitle") { source.push_str(&format!(" In *{}*.", booktitle.trim_matches(|c| c == '{' || c == '}' || c == '"'))); } else if let Some(howpublished) = find_tag("howpublished") { source.push_str(&format!(" {}.", howpublished)); }
    parts.push(source);
    parts.iter().filter(|s| !s.is_empty() && *s != ".").cloned().collect::<Vec<_>>().join(" ")
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
    // MODIFICATION 1: New parameter for link prefix
    bibliography_link_prefix: &str,
) -> Result<ProcessingOutput, JsValue> {

    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();

    // --- 1. Define Regex & Find Unique Citations ---
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
    log!("Found {} unique citation keys.", unique_citations.len());

    // --- 2. Parse BibTeX ---
    let bibtex_data = Bibtex::parse(bibtex_input)
        .map_err(|e| JsValue::from_str(&format!("BibTeX parsing error: {:?}", e)))?;
    let all_bib_entries = bibtex_data.bibliographies();
    log!("Parsed {} BibTeX entries.", all_bib_entries.len());

    // --- 3. Group BibTeX entries by (first_author_lastname_lc, year_yy) & Sort by Title ---
    let mut grouped_entries: HashMap<(String, String), Vec<&Bibliography>> = HashMap::new();
    for entry in all_bib_entries {
        if let (Some(author_str), Some(year_str)) = (entry.tags().iter().find(|(k,_)| k.eq_ignore_ascii_case("author")).map(|(_,v)| v),
                                                    entry.tags().iter().find(|(k,_)| k.eq_ignore_ascii_case("year")).map(|(_,v)| v)) {
            if let (Some(first_last_name_lc), Some(year_yy)) = (get_first_author_last_name(author_str), get_year_yy(year_str)) {
                 grouped_entries.entry((first_last_name_lc, year_yy)).or_default().push(entry);
            }
        }
    }
    for group in grouped_entries.values_mut() {
        group.sort_by(|a, b| get_entry_title_for_sort(a).cmp(&get_entry_title_for_sort(b)));
    }

    // --- 4. Map Markdown keys to specific BibTeX entries (Exact & Fuzzy Matching) ---
    let mut final_entry_map: HashMap<String, &Bibliography> = HashMap::new(); // MD Key -> Bib Entry Ref
    let mut missing_keys: HashSet<String> = unique_citations.keys().cloned().collect();

    for (md_key, (author_part, year_part, suffix_part)) in &unique_citations {
        let md_author_lc = author_part.to_lowercase();
        let lookup_key = (md_author_lc.clone(), year_part.clone());
        let mut found_match = false;

        // --- 4a. Try Exact Match ---
        if let Some(candidate_group) = grouped_entries.get(&lookup_key) {
            let index = suffix_to_index(suffix_part);
            if let Some(selected_entry) = candidate_group.get(index) {
                final_entry_map.insert(md_key.clone(), selected_entry);
                missing_keys.remove(md_key);
                found_match = true;
                log!("Mapped key '{}' (Exact Match).", md_key);
            } else {
                log!("Warning: Suffix '{}' for key '{}' is out of bounds (exact match group size {}).", suffix_part, md_key, candidate_group.len());
            }
        }

        // --- 4b. Try Fuzzy Match if Exact Failed ---
        if !found_match {
            let mut best_fuzzy_match: Option<(usize, String, Vec<&Bibliography>)> = None; // (distance, matched_author_lc, group)

            for entry in all_bib_entries {
                 // Check if year matches
                 if let Some(entry_year_yy) = entry.tags().iter().find(|(k,_)| k.eq_ignore_ascii_case("year")).map(|(_,v)| get_year_yy(v)).flatten() {
                    if entry_year_yy == *year_part {
                         // Year matches, check author distance
                         if let Some(entry_author_str) = entry.tags().iter().find(|(k,_)| k.eq_ignore_ascii_case("author")).map(|(_,v)| v) {
                             if let Some(entry_lastname_lc) = get_first_author_last_name(entry_author_str) {
                                 let distance = levenshtein(&md_author_lc, &entry_lastname_lc);

                                 if distance <= FUZZY_MATCH_THRESHOLD {
                                     // Potential fuzzy match, check if it's better than current best
                                     if best_fuzzy_match.is_none() || distance < best_fuzzy_match.as_ref().unwrap().0 {
                                         // Found a new best distance, retrieve the *full group* for this potential author
                                         if let Some(group) = grouped_entries.get(&(entry_lastname_lc.clone(), year_part.clone())) {
                                              best_fuzzy_match = Some((distance, entry_lastname_lc, group.clone())); // Clone group ref vec
                                              log!("Potential fuzzy match for '{}': '{}' distance {} (Year {})", md_key, group[0].citation_key(), distance, year_part);
                                         }
                                     }
                                 }
                             }
                         }
                    }
                }
            } // End loop through all bib entries for fuzzy check

            // --- 4c. Process Best Fuzzy Match ---
            if let Some((dist, matched_author, group)) = best_fuzzy_match {
                log!("Applying fuzzy match for key '{}': Closest author='{}' (dist={})", md_key, matched_author, dist);
                let index = suffix_to_index(suffix_part);
                if let Some(selected_entry) = group.get(index) {
                    final_entry_map.insert(md_key.clone(), selected_entry);
                    missing_keys.remove(md_key);
                    // Note: We don't set found_match = true here, it remains false from the exact check phase
                } else {
                    log!("Warning: Suffix '{}' for key '{}' is out of bounds (fuzzy match group size {}).", suffix_part, md_key, group.len());
                }
            }
        } // End if !found_match (fuzzy check)

        // --- 4d. Log if still missing ---
        if !final_entry_map.contains_key(md_key) {
              log!("Warning: Could not map key '{}' (missing or suffix out of bounds).", md_key);
        }

    } // End loop through unique_citations

    // --- 5. Generate Bibliography (Deduplicated) ---
    let mut bibliography_markdown_lines: Vec<String> = Vec::new();
    bibliography_markdown_lines.push("# Bibliography".to_string());
    bibliography_markdown_lines.push("".to_string());

    // MODIFICATION 2: Deduplication Logic
    let mut rendered_bib_keys = HashSet::new(); // Track BibTeX citation keys already rendered
    let mut bibliography_items_to_render : Vec<(&String, &Bibliography)> = Vec::new(); // (Markdown Key, Bib Entry Ref)

    // Collect unique entries to render, preferring keys without suffix or 'a' for anchor generation
    let mut sorted_unique_citations_keys: Vec<String> = unique_citations.keys().cloned().collect();
     // Sort primarily by author/year, then by suffix index to process '@Smith20'/'@Smith20a' before '@Smith20b'
    sorted_unique_citations_keys.sort_by(|a, b| {
        let (author_a, year_a, suffix_a) = unique_citations.get(a).unwrap();
        let (author_b, year_b, suffix_b) = unique_citations.get(b).unwrap();
        author_a.to_lowercase().cmp(&author_b.to_lowercase())
            .then_with(|| year_a.cmp(year_b))
            .then_with(|| suffix_to_index(suffix_a).cmp(&suffix_to_index(suffix_b)))
    });

    for md_key in &sorted_unique_citations_keys {
        if let Some(entry) = final_entry_map.get(md_key) {
             // Use the BibTeX citation key for deduplication check
             if rendered_bib_keys.insert(entry.citation_key().to_string()) {
                 // This BibTeX entry hasn't been added yet, add it to our render list
                 bibliography_items_to_render.push((md_key, entry));
             }
        }
    }

    // Sort the final list for bibliography output (e.g., by author/year/title)
    bibliography_items_to_render.sort_by(|(key_a, entry_a), (key_b, entry_b)|{
         let (author_a_part, year_a_part, suffix_a_part) = unique_citations.get(*key_a).unwrap();
         let (author_b_part, year_b_part, suffix_b_part) = unique_citations.get(*key_b).unwrap();
         // Sort primarily by Author, Year, then Title (ensures consistent ordering)
         author_a_part.to_lowercase().cmp(&author_b_part.to_lowercase())
            .then_with(|| year_a_part.cmp(year_b_part))
            .then_with(|| get_entry_title_for_sort(entry_a).cmp(&get_entry_title_for_sort(entry_b)))
            // Fallback sort by suffix index if all else is equal (unlikely but possible)
            .then_with(|| suffix_to_index(suffix_a_part).cmp(&suffix_to_index(suffix_b_part)))
    });


    // Generate bibliography sections from the deduplicated, sorted list
    for (md_key, entry) in &bibliography_items_to_render {
        let formatted_entry = format_bib_entry_for_markdown(entry);
        let (author_part, year_part, suffix_part) = unique_citations.get(*md_key).unwrap();
        // Anchor uses the components from the *specific markdown key* that caused this entry to be included
        let anchor = create_anchor(author_part, year_part, suffix_part);

        let heading = format!("## <a name=\"{}\"></a>{}", anchor, formatted_entry);
        bibliography_markdown_lines.push(heading);
        bibliography_markdown_lines.push("".to_string());
    }


    if bibliography_items_to_render.is_empty() { /* Add messages */
        if !missing_keys.is_empty() { bibliography_markdown_lines.push("*(No BibTeX entries found matching any citation keys)*".to_string()); }
        else { bibliography_markdown_lines.push("*(No citation keys found in Markdown input)*".to_string()); }
    }
    let bibliography_content = bibliography_markdown_lines.join("\n");


    // --- 6. Replace citations in Markdown ---
    let modified_markdown_content = citation_regex.replace_all(markdown_input, |caps: &Captures| {
        let full_match = caps.get(1).map_or("", |m| m.as_str()); // @AuthorYY[suffix]

        if final_entry_map.contains_key(full_match) {
             let author_part = caps.get(2).map_or("", |m| m.as_str());
             let year_part = caps.get(3).map_or("", |m| m.as_str());
             let suffix_part = caps.get(4).map_or("", |m| m.as_str());

             let anchor = create_anchor(author_part, year_part, suffix_part); // Suppresses 'a'

             let link_text = if suffix_part.is_empty() || suffix_part == "a" {
                 format!("{}{}", author_part, year_part) // Suppress 'a'
             } else {
                 format!("{}{}{}", author_part, year_part, suffix_part)
             };

             // MODIFICATION 1: Use prefix + fixed filename
             format!("[{}]({}#{})", link_text, bibliography_link_prefix, anchor)
        } else {
            if !missing_keys.contains(full_match){ log!("Warning: Unmapped key '{}' found.", full_match); }
            format!("{} [Reference Not Found]", full_match)
        }
    }).to_string();


    log!("Markdown processing complete.");

    Ok(ProcessingOutput {
        modified_markdown: modified_markdown_content,
        bibliography_markdown: bibliography_content,
    })
}
