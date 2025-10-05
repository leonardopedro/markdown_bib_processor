use hayagriva::io::from_biblatex_str;
use hayagriva::{
    BibliographyDriver, BibliographyRequest, CitationItem, CitationRequest, Entry,
};
use hayagriva::citationberg::{IndependentStyle, Locale, LocaleFile};
use hayagriva::types::Person;
use regex::{Captures, Regex};
use std::collections::{HashMap, HashSet};

// For fuzzy matching
use levenshtein::levenshtein;

pub struct ProcessingOutput {
    pub modified_markdown: String,
    pub bibliography_markdown: String,
}

pub fn process_markdown_and_bibtex(
    markdown_input: &str,
    bibtex_input: &str,
    bibliography_link_prefix: &str,
    csl_style: &str,
    locale: &str,
) -> Result<ProcessingOutput, String> {
    // --- 1. Define Regex & Find Unique Citations ---
    let citation_regex = Regex::new(r"(@([a-zA-Z]+)(\d{2})([a-z]?))\b")
        .map_err(|e| format!("Regex compilation error: {}", e))?;

    let mut unique_citations: HashMap<String, (String, String, String)> = HashMap::new();
    for cap in citation_regex.captures_iter(markdown_input) {
        let full_match = cap.get(1).map_or("", |m| m.as_str()).to_string();
        let author_part = cap.get(2).map_or("", |m| m.as_str()).to_string();
        let year_part = cap.get(3).map_or("", |m| m.as_str()).to_string();
        let suffix_part = cap.get(4).map_or("", |m| m.as_str()).to_string();
        if !full_match.is_empty() {
            unique_citations
                .entry(full_match)
                .or_insert((author_part, year_part, if suffix_part==""{"a".to_string()} else {suffix_part}));
        }
    }

    // --- 2. Parse BibTeX using Hayagriva ---
    let bib_entries = from_biblatex_str(bibtex_input)
        .map_err(|e| format!("BibTeX parsing error: {:?}", e))
        .unwrap_or_default();

    // --- 3. Group BibTeX entries by (first_author_lastname_lc, year_yy) & Sort by Title ---
    let mut grouped_entries: HashMap<(String, String), Vec<&Entry>> = HashMap::new();
    for entry in &bib_entries {
        if let (Some(first_last_name_lc), Some(year_yy)) =
            (get_first_author_last_name(entry), get_year_yy(entry))
        {
            grouped_entries
                .entry((first_last_name_lc, year_yy))
                .or_default()
                .push(entry);
        }
    }
    for group in grouped_entries.values_mut() {
        group.sort_by(|a, b| get_entry_title_for_sort(a).cmp(&get_entry_title_for_sort(b)));
    }

    // --- 4. Map Markdown keys to specific BibTeX entries (Exact & Fuzzy Matching) ---
    let mut final_entry_map: HashMap<String, &Entry> = HashMap::new(); // MD Key -> Bib Entry Ref
    let mut missing_keys: HashSet<String> = unique_citations.keys().cloned().collect();

    for (md_key, (author_part, year_part, suffix_part)) in &unique_citations {
        let md_author_lc = author_part.to_lowercase();
        let lookup_key = (md_author_lc.clone(), year_part.clone());
        let mut found_match = false;

        // --- 4a. Try Exact Match ---
        if let Some(candidate_group) = grouped_entries.get(&lookup_key) {
            let index = suffix_to_index(suffix_part);
            if let Some(selected_entry) = candidate_group.get(index) {
                final_entry_map.insert(author_part.clone()+year_part+if suffix_part=="a"{""} else {suffix_part}, selected_entry);
                missing_keys.remove(md_key);
                found_match = true;
            }
        }

        // --- 4b. Try Fuzzy Match if Exact Failed ---
        if !found_match {
            const FUZZY_MATCH_THRESHOLD: usize = 2; // Stricter threshold
            let mut best_fuzzy_match: Option<(usize, &Entry)> = None;

            for entry in &bib_entries {
                if let Some(entry_year_yy) = get_year_yy(entry) {
                    if entry_year_yy == *year_part {
                        if let Some(entry_lastname_lc) = get_first_author_last_name(entry) {
                            let distance = levenshtein(&md_author_lc, &entry_lastname_lc);
                            if distance <= FUZZY_MATCH_THRESHOLD
                                && (best_fuzzy_match.is_none()
                                    || distance < best_fuzzy_match.as_ref().unwrap().0)
                            {
                                best_fuzzy_match = Some((distance, entry));
                            }
                        }
                    }
                }
            }

            if let Some((_dist, selected_entry)) = best_fuzzy_match {
                final_entry_map.insert(author_part.clone()+year_part+(if suffix_part=="a"{""} else {suffix_part}), selected_entry);
                missing_keys.remove(md_key);
            }
        }
    }

    // --- 5. Generate Bibliography (Deduplicated and Sorted) ---
    let style = IndependentStyle::from_xml(csl_style)
        .map_err(|e| format!("CSL parsing error: {}", e))?;
    let locale_file = LocaleFile::from_xml(locale)
        .map_err(|e| format!("Locale parsing error: {}", e))?;
    let locales = [locale_file.into()];

    let mut bibliography_markdown_lines: Vec<String> = Vec::new();
    bibliography_markdown_lines.push("### Bibliography".to_string());
    bibliography_markdown_lines.push("".to_string());

    let mut used_bib_keys: HashSet<String> = HashSet::new();
    let mut bibliography_items_to_render: Vec<(&Entry,&String)> = Vec::new();
    
    // Sort keys to ensure deterministic order
    let mut sorted_md_keys: Vec<&String> = final_entry_map.keys().collect();
    sorted_md_keys.sort();

    for md_key in sorted_md_keys {
        if let Some(entry) = final_entry_map.get(md_key) {
            if used_bib_keys.insert(entry.key().to_string()) {
                bibliography_items_to_render.push( (entry,md_key) ) ;
            }
        }
    }

    bibliography_items_to_render.sort_by(|a, b| {
        let author_a = get_authors_string(a.0);
        let author_b = get_authors_string(b.0);
        let year_a = a.0.date().map(|d| d.year.to_string()).unwrap_or_default();
        let year_b = b.0.date().map(|d| d.year.to_string()).unwrap_or_default();

        author_a
            .cmp(&author_b)
            .then_with(|| year_a.cmp(&year_b))
            .then_with(|| get_entry_title_for_sort(a.0).cmp(&get_entry_title_for_sort(b.0)))
    });

    for entry in bibliography_items_to_render.iter() {
        let formatted_entry = format_bib_entry_for_markdown(entry.0, &style, &locales);
        let line = format!(
            "#### {}<a href=\"#{}\" id=\"{}\"></a>",
            formatted_entry,
            entry.1,
            entry.1
        );
        bibliography_markdown_lines.push(line);
    }

    let bibliography_content = if bibliography_items_to_render.is_empty() {
        "### Bibliography".to_string()
    } else {
        bibliography_markdown_lines.join("\n")
    };

    // --- 6. Replace citations in Markdown ---
    let mut citation_indices: HashMap<String, usize> = HashMap::new();
    for (i, entry) in bibliography_items_to_render.iter().enumerate() {
        citation_indices.insert(entry.0.key().to_string(), i + 1);
    }

    let modified_markdown_content = citation_regex
        .replace_all(markdown_input, |caps: &Captures| {
           let full_match = caps.get(1).map_or("", |m| m.as_str()).to_string();
           let author_part = caps.get(2).map_or("", |m| m.as_str()).to_string();
           let year_part = caps.get(3).map_or("", |m| m.as_str()).to_string();
           let suffix_part = caps.get(4).map_or("", |m| m.as_str()).to_string();
           let anchor = author_part.clone()+&year_part+(if suffix_part=="a"{""} else {&suffix_part});

            if let Some(entry) = final_entry_map.get(&anchor) {
                if let Some(_index) = citation_indices.get(entry.key()) {
                    let link = format!("[[{}]]({}#{})", anchor, bibliography_link_prefix, anchor);
                    return link;
                }
            }
            full_match.to_string()
        })
        .to_string();

    Ok(ProcessingOutput {
        modified_markdown: modified_markdown_content,
        bibliography_markdown: bibliography_content,
    })
}

// --- Helper Functions ---

// CORRECTED
fn get_first_author_last_name(entry: &Entry) -> Option<String> {
    entry
        .authors()
        .and_then(|authors| authors.get(0))
        .map(|person| person.name.to_lowercase())
}

fn get_year_yy(entry: &Entry) -> Option<String> {
    entry.date().and_then(|date| {
        let year_str = date.year.to_string();
        if year_str.len() >= 2 {
            Some(year_str.chars().skip(year_str.len() - 2).collect())
        } else {
            None
        }
    })
}

// CORRECTED
fn person_to_string(p: &Person) -> String {
    format!(
        "{}{}",
        &p.name,
        p.given_name.as_deref().unwrap_or("")
    )
}

fn get_authors_string(entry: &Entry) -> String {
    entry
        .authors()
        .map(|authors| {
            authors
                .iter()
                .map(person_to_string)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_else(|| "Anonymous".to_string())
}

fn get_entry_title_for_sort(entry: &Entry) -> String {
    entry
        .title()
        .map(|t| t.to_string().to_lowercase())
        .unwrap_or_default()
}

fn suffix_to_index(suffix: &str) -> usize {
    if suffix.is_empty() {
        0
    } else {
        (suffix.chars().next().unwrap_or('a') as u32).saturating_sub('a' as u32) as usize
    }
}

fn format_bib_entry_for_markdown(
    entry: &Entry,
    style: &IndependentStyle,
    locales: &[Locale],
) -> String {
    let mut driver = BibliographyDriver::new();
    driver.citation(CitationRequest::from_items(
        vec![CitationItem::with_entry(entry)],
        style,
        locales,
    ));

    let request = BibliographyRequest { style, locale: None, locale_files: locales };
    let result = driver.finish(request);

    result
        .bibliography
        .and_then(|bib| bib.items.into_iter().next())
        .map(|item| item.content.to_string())
        .unwrap_or_default()
}