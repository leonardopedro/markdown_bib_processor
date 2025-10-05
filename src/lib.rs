use regex::{Regex, Captures};
use nom_bibtex::{Bibtex, Bibliography};
use std::collections::{HashMap, HashSet};
// For fuzzy matching
use levenshtein::levenshtein;

use once_cell::sync::Lazy;



// A regex to check for content that is only whitespace or other emphasis markers.
static MEANINGLESS_CONTENT_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[\s_~*`]*$").unwrap());


#[derive(Debug)]
pub struct ProcessingOutput {
    modified_markdown: String,
    bibliography_markdown: String,
}

impl ProcessingOutput {
    pub fn modified_markdown(&self) -> &str { &self.modified_markdown }
    pub fn bibliography_markdown(&self) -> &str { &self.bibliography_markdown }
}

pub fn process_markdown_and_bibtex(
    markdown_input: &str,
    bibtex_input: &str,
    bibliography_link_prefix: &str,
) -> Result<ProcessingOutput, String> {

    // --- 1. Define Regex & Find Unique Citations ---
    let citation_regex = Regex::new(r"(@([a-zA-Z]+)(\d{2})([a-z]?))\b")
        .map_err(|e| format!("Regex error: {}", e))?;

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

    // --- 2. Parse BibTeX ---
    let bibtex_data = Bibtex::parse(bibtex_input)
        .map_err(|e| format!("BibTeX parsing error: {:?}", e))?;
    let all_bib_entries = bibtex_data.bibliographies();

    // --- 3. Group BibTeX entries by (first_author_lastname_lc, year_yy) & Sort by Title ---
    let mut grouped_entries: HashMap<(String, String), Vec<&Bibliography>> = HashMap::new();
    for entry in all_bib_entries {
        if let (Some(author_str), Some(year_str)) = (get_tag_content(entry, "author"), get_tag_content(entry, "year")) {
            if let (Some(first_last_name_lc), Some(year_yy)) = (get_first_author_last_name(&author_str), get_year_yy(&year_str)) {
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
            } 
        }

        // --- 4b. Try Fuzzy Match if Exact Failed ---
        if !found_match {
            const FUZZY_MATCH_THRESHOLD: usize = 10;
            let mut best_fuzzy_match: Option<(usize, String, Vec<&Bibliography>)> = None; // (distance, matched_author_lc, group)

            for entry in all_bib_entries {
                 if let Some(entry_year_yy) = get_tag_content(entry, "year").and_then(|y| get_year_yy(&y)) {
                    if entry_year_yy == *year_part {
                         if let Some(entry_author_str) = get_tag_content(entry, "author") {
                             if let Some(entry_lastname_lc) = get_first_author_last_name(&entry_author_str) {
                                 let distance = levenshtein(&md_author_lc, &entry_lastname_lc);

                                 if distance <= FUZZY_MATCH_THRESHOLD {
                                     if best_fuzzy_match.is_none() || distance < best_fuzzy_match.as_ref().unwrap().0 {
                                         if let Some(group) = grouped_entries.get(&(entry_lastname_lc.clone(), year_part.clone())) {
                                              best_fuzzy_match = Some((distance, entry_lastname_lc, group.clone())); 
                                         }
                                     }
                                 }
                             }
                         }
                    }
                }
            } 

            // --- 4c. Process Best Fuzzy Match ---
            if let Some((_dist, _matched_author, group)) = best_fuzzy_match {
                let index = suffix_to_index(suffix_part);
                if let Some(selected_entry) = group.get(index) {
                    final_entry_map.insert(md_key.clone(), selected_entry);
                    missing_keys.remove(md_key);
                } 
            }
        } 
    } 

    // --- 5. Generate Bibliography (Deduplicated) ---
    let mut bibliography_markdown_lines: Vec<String> = Vec::new();
    bibliography_markdown_lines.push("# Bibliography".to_string());
    bibliography_markdown_lines.push("".to_string());

    let mut rendered_bib_keys = HashSet::new(); 
    let mut bibliography_items_to_render : Vec<(&String, &Bibliography)> = Vec::new();

    let mut sorted_unique_citations_keys: Vec<String> = unique_citations.keys().cloned().collect();
    sorted_unique_citations_keys.sort_by(|a, b| {
        let (author_a, year_a, suffix_a) = unique_citations.get(a).unwrap();
        let (author_b, year_b, suffix_b) = unique_citations.get(b).unwrap();
        author_a.to_lowercase().cmp(&author_b.to_lowercase())
            .then_with(|| year_a.cmp(year_b))
            .then_with(|| suffix_to_index(suffix_a).cmp(&suffix_to_index(suffix_b)))
    });

    for md_key in &sorted_unique_citations_keys {
        if let Some(entry) = final_entry_map.get(md_key) {
             if rendered_bib_keys.insert(entry.citation_key().to_string()) {
                 bibliography_items_to_render.push((md_key, entry));
             }
        }
    }

    bibliography_items_to_render.sort_by(|(_key_a, entry_a), (_key_b, entry_b)|{
         let author_a = get_tag_content(entry_a, "author").unwrap_or_default();
         let author_b = get_tag_content(entry_b, "author").unwrap_or_default();
         let year_a = get_tag_content(entry_a, "year").unwrap_or_default();
         let year_b = get_tag_content(entry_b, "year").unwrap_or_default();

         get_first_author_last_name(&author_a).cmp(&get_first_author_last_name(&author_b))
            .then_with(|| year_a.cmp(&year_b))
            .then_with(|| get_entry_title_for_sort(entry_a).cmp(&get_entry_title_for_sort(entry_b)))
    });

    for (md_key, entry) in &bibliography_items_to_render {
        let formatted_entry = format_bib_entry_for_markdown(entry);
        let (author_part, year_part, suffix_part) = unique_citations.get(*md_key).unwrap();
        let anchor = create_anchor(author_part, year_part, suffix_part);

        let heading = format!("## <a name=\"{}\"></a>{}", anchor, formatted_entry);
        bibliography_markdown_lines.push(heading);
        bibliography_markdown_lines.push("".to_string());
    }

    if bibliography_items_to_render.is_empty() {
        if !missing_keys.is_empty() { bibliography_markdown_lines.push("*(No BibTeX entries found matching any citation keys)*".to_string()); }
        else { bibliography_markdown_lines.push("*(No citation keys found in Markdown input)*".to_string()); }
    }
    let bibliography_content = bibliography_markdown_lines.join("\n");

    // --- 6. Replace citations in Markdown ---
    let modified_markdown_content = citation_regex.replace_all(markdown_input, |caps: &Captures| {
        let full_match = caps.get(1).map_or("", |m| m.as_str()); 

        if final_entry_map.contains_key(full_match) {
             let author_part = caps.get(2).map_or("", |m| m.as_str());
             let year_part = caps.get(3).map_or("", |m| m.as_str());
             let suffix_part = caps.get(4).map_or("", |m| m.as_str());

             let anchor = create_anchor(author_part, year_part, suffix_part);

             let link_text = if suffix_part.is_empty() || suffix_part == "a" {
                 format!("{}{}", author_part, year_part) 
             } else {
                 format!("{}{}{}", author_part, year_part, suffix_part)
             };

             format!("[{}]({}#{})", link_text, bibliography_link_prefix, anchor)
        } else {
            format!("{} [Reference Not Found]", full_match)
        }
    }).to_string();

    Ok(ProcessingOutput {
        modified_markdown: modified_markdown_content,
        bibliography_markdown: bibliography_content,
    })
}

// --- Helper Functions ---

fn get_tag_content(entry: &Bibliography, tag: &str) -> Option<String> {
    entry.tags().iter().find(|(k, _)| k.eq_ignore_ascii_case(tag)).map(|(_, v)| v.clone())
}

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
    get_tag_content(entry, "title")
        .map(|v| v.trim_matches(|c| c == '{' || c == '}' || c == '"').to_lowercase())
        .unwrap_or_else(|| "".to_string())
}

fn create_anchor(author_part: &str, year_part: &str, suffix_part: &str) -> String {
    let base = format!("{}{}", author_part, year_part).to_lowercase();
    if suffix_part.is_empty() || suffix_part == "a" { base }
    else { format!("{}{}", base, suffix_part) }
}

fn format_bib_entry_for_markdown(entry: &Bibliography) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(author) = get_tag_content(entry, "author") { parts.push(author.replace(" and ", ", ")); } else { parts.push("Unknown Author".to_string()); }
    if let Some(year) = get_tag_content(entry, "year") { parts.push(format!("({})", year)); } else { parts.push("(N.D.)".to_string()); }
    if let Some(title) = get_tag_content(entry, "title") { let clean_title = title.trim_matches(|c| c == '{' || c == '}' || c == '"'); parts.push(format!("*{}.*", clean_title)); } else { parts.push("*No Title*.".to_string()); }
    let mut source = String::new();
    if let Some(journal) = get_tag_content(entry, "journal") { source.push_str(&format!(" *{}*", journal.trim_matches(|c| c == '{' || c == '}' || c == '"'))); if let Some(volume) = get_tag_content(entry, "volume") { source.push_str(&format!(", {}", volume)); } if let Some(pages) = get_tag_content(entry, "pages") { source.push_str(&format!(", pp. {}", pages.replace("--", "-"))); } source.push('.'); } else if let Some(booktitle) = get_tag_content(entry, "booktitle") { source.push_str(&format!(" In *{}*.", booktitle.trim_matches(|c| c == '{' || c == '}' || c == '"'))); } else if let Some(howpublished) = get_tag_content(entry, "howpublished") { source.push_str(&format!(" {}.", howpublished)); }
    parts.push(source);
    parts.iter().filter(|s| !s.is_empty() && *s != ".").cloned().collect::<Vec<_>>().join(" ")
}
