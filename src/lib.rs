use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;
use regex::{Regex, Captures};
use nom_bibtex::{Bibtex, Bibliography};
use std::collections::{HashMap, HashSet};
// For fuzzy matching
use levenshtein::levenshtein;

use once_cell::sync::Lazy;

// Statically compile regex patterns for performance.
// `once_cell::sync::Lazy` ensures this is done only once, safely across threads.
static LINK_IMAGE_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(!?\[)([^\]]*?)$").unwrap());
static BOLD_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(\*\*)([^*]*?)$").unwrap());
static ITALIC_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(__)([^_]*?)$").unwrap());
static BOLD_ITALIC_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(\*\*\*)([^*]*?)$").unwrap());
static SINGLE_ASTERISK_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(\*)([^*]*?)$").unwrap());
static SINGLE_UNDERSCORE_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(_)([^_]*?)$").unwrap());
static INLINE_CODE_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(`)([^`]*?)$").unwrap());
static STRIKETHROUGH_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(~~)([^~]*?)$").unwrap());

// A regex to check for content that is only whitespace or other emphasis markers.
static MEANINGLESS_CONTENT_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[\s_~*`]*$").unwrap());

/// Helper function to check if we have a complete code block.
fn has_complete_code_block(text: &str) -> bool {
    let triple_backticks = text.matches("```").count();
    triple_backticks > 0 && triple_backticks % 2 == 0 && text.contains('\n')
}

/// Handles incomplete links and images by preserving them with a special marker.
fn handle_incomplete_links_and_images(text: &str) -> String {
    if let Some(captures) = LINK_IMAGE_PATTERN.captures(text) {
        let link_match = captures.get(0).unwrap();
        let is_image = captures.get(1).unwrap().as_str().starts_with('!');

        // For images, remove them as they can't show a skeleton UI.
        if is_image {
            return text[..link_match.start()].to_string();
        }

        // For links, preserve the text and close the link with a special placeholder.
        return format!("{text}](streamdown:incomplete-link)");
    }

    text.to_string()
}

/// Completes incomplete bold formatting (**).
fn handle_incomplete_bold(text: &str) -> String {
    if has_complete_code_block(text) {
        return text.to_string();
    }

    if let Some(captures) = BOLD_PATTERN.captures(text) {
        let content_after_marker = captures.get(2).unwrap().as_str();
        if content_after_marker.is_empty()
            || MEANINGLESS_CONTENT_PATTERN.is_match(content_after_marker)
        {
            return text.to_string();
        }

        let asterisk_pairs = text.matches("**").count();
        if asterisk_pairs % 2 == 1 {
            return format!("{text}**");
        }
    }

    text.to_string()
}

/// Completes incomplete italic formatting with double underscores (__).
fn handle_incomplete_double_underscore_italic(text: &str) -> String {
    if let Some(captures) = ITALIC_PATTERN.captures(text) {
        let content_after_marker = captures.get(2).unwrap().as_str();
        if content_after_marker.is_empty()
            || MEANINGLESS_CONTENT_PATTERN.is_match(content_after_marker)
        {
            return text.to_string();
        }

        let underscore_pairs = text.matches("__").count();
        if underscore_pairs % 2 == 1 {
            return format!("{text}__");
        }
    }

    text.to_string()
}

/// Counts single asterisks that are not part of double asterisks or list markers.
fn count_single_asterisks(text: &str) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let mut count = 0;
    for i in 0..chars.len() {
        if chars[i] == '*' {
            let prev_char = chars.get(i.wrapping_sub(1));
            let next_char = chars.get(i + 1);

            // Skip if part of ** or *** etc.
            if (prev_char.map_or(false, |&c| c == '*'))
                || (next_char.map_or(false, |&c| c == '*'))
            {
                continue;
            }

            // Skip if it's a list marker.
            let line_start = text[..i].rfind('\n').map_or(0, |pos| pos + 1);
            let before_asterisk = &text[line_start..i];
            if before_asterisk.trim().is_empty() && next_char.map_or(false, |&c| c.is_whitespace())
            {
                continue;
            }

            count += 1;
        }
    }
    count
}

/// Completes incomplete italic formatting with single asterisks (*).
fn handle_incomplete_single_asterisk_italic(text: &str) -> String {
    if has_complete_code_block(text) {
        return text.to_string();
    }
    
    if let Some(captures) = SINGLE_ASTERISK_PATTERN.captures(text) {
        let content_after_marker = captures.get(2).unwrap().as_str();
        if content_after_marker.is_empty()
            || MEANINGLESS_CONTENT_PATTERN.is_match(content_after_marker)
        {
            return text.to_string();
        }

        if count_single_asterisks(text) % 2 == 1 {
            return format!("{text}*");
        }
    }
    
    text.to_string()
}

/// Checks if a character position is within a math block ($ or $$).
fn is_within_math_block(text: &str, position: usize) -> bool {
    let mut in_inline_math = false;
    let mut in_block_math = false;
    let mut chars = text.chars().enumerate().peekable();

    while let Some((i, ch)) = chars.next() {
        if i >= position {
            break;
        }
        if ch == '\\' && chars.peek().map_or(false, |&(_, next_ch)| next_ch == '$') {
            chars.next(); // Skip escaped dollar sign
            continue;
        }
        if ch == '$' {
            if chars.peek().map_or(false, |&(_, next_ch)| next_ch == '$') {
                in_block_math = !in_block_math;
                chars.next(); // Skip second dollar sign
                in_inline_math = false;
            } else if !in_block_math {
                in_inline_math = !in_inline_math;
            }
        }
    }

    in_inline_math || in_block_math
}


/// Counts single underscores not part of double underscores or inside math blocks.
fn count_single_underscores(text: &str) -> usize {
    text.char_indices()
        .filter(|&(i, ch)| {
            if ch == '_' {
                // Not part of __
                let prev_char = text.chars().nth(i.saturating_sub(1));
                let next_char = text.chars().nth(i + 1);
                if prev_char == Some('_') || next_char == Some('_') {
                    return false;
                }
                
                // Not escaped
                if prev_char == Some('\\') {
                    return false;
                }

                // Not inside math block
                if is_within_math_block(text, i) {
                    return false;
                }
                
                // Not word-internal
                if let (Some(p), Some(n)) = (prev_char, next_char) {
                    if (p.is_alphanumeric() || p == '_') && (n.is_alphanumeric() || n == '_') {
                        return false;
                    }
                }
                
                return true;
            }
            false
        })
        .count()
}

/// Completes incomplete italic formatting with single underscores (_).
fn handle_incomplete_single_underscore_italic(text: &str) -> String {
    if has_complete_code_block(text) {
        return text.to_string();
    }

    if let Some(captures) = SINGLE_UNDERSCORE_PATTERN.captures(text) {
        let content_after_marker = captures.get(2).unwrap().as_str();
        if content_after_marker.is_empty()
            || MEANINGLESS_CONTENT_PATTERN.is_match(content_after_marker)
        {
            return text.to_string();
        }

        if count_single_underscores(text) % 2 == 1 {
            return format!("{text}_");
        }
    }
    
    text.to_string()
}

/// Checks if a backtick is part of a triple-backtick sequence.
fn is_part_of_triple_backtick(text: &str, i: usize) -> bool {
    (text.len() >= i + 3 && &text[i..i + 3] == "```")
        || (i > 0 && text.len() >= i + 2 && &text[i - 1..i + 2] == "```")
        || (i > 1 && &text[i - 2..i + 1] == "```")
}

/// Counts single backticks that are not part of triple backticks.
fn count_single_backticks(text: &str) -> usize {
    text.char_indices()
        .filter(|&(i, c)| c == '`' && !is_part_of_triple_backtick(text, i))
        .count()
}

/// Completes incomplete inline code formatting (`).
fn handle_incomplete_inline_code(text: &str) -> String {
    let all_triple_backticks = text.matches("```").count();
    let inside_incomplete_code_block = all_triple_backticks % 2 == 1;

    if all_triple_backticks > 0 && all_triple_backticks % 2 == 0 && text.contains('\n') {
        return text.to_string();
    }

    if let Some(_captures) = INLINE_CODE_PATTERN.captures(text) {
        if !inside_incomplete_code_block && count_single_backticks(text) % 2 == 1 {
            return format!("{text}`");
        }
    }

    text.to_string()
}

/// Completes incomplete strikethrough formatting (~~).
fn handle_incomplete_strikethrough(text: &str) -> String {
    if let Some(captures) = STRIKETHROUGH_PATTERN.captures(text) {
        let content_after_marker = captures.get(2).unwrap().as_str();
        if content_after_marker.is_empty()
            || MEANINGLESS_CONTENT_PATTERN.is_match(content_after_marker)
        {
            return text.to_string();
        }

        let tilde_pairs = text.matches("~~").count();
        if tilde_pairs % 2 == 1 {
            return format!("{text}~~");
        }
    }

    text.to_string()
}

/// Completes incomplete block KaTeX formatting ($$).
fn handle_incomplete_block_katex(text: &str) -> String {
    let dollar_pairs = text.matches("$$").count();
    if dollar_pairs % 2 == 1 {
        if let Some(first_dollar_index) = text.find("$$") {
            let has_newline_after_start = text[first_dollar_index..].contains('\n');
            if has_newline_after_start && !text.ends_with('\n') {
                return format!("{text}\n$$");
            }
        }
        return format!("{text}$$");
    }
    text.to_string()
}

/// Completes incomplete bold-italic formatting (***).
fn handle_incomplete_bold_italic(text: &str) -> String {
    if has_complete_code_block(text) {
        return text.to_string();
    }
    // This check prevents cases like **** from being treated as incomplete ***
    if text.starts_with("****") {
        return text.to_string();
    }

    if let Some(captures) = BOLD_ITALIC_PATTERN.captures(text) {
        let content_after_marker = captures.get(2).unwrap().as_str();
        if content_after_marker.is_empty()
            || MEANINGLESS_CONTENT_PATTERN.is_match(content_after_marker)
        {
            return text.to_string();
        }
        
        let triple_asterisk_count = text.matches("***").count();
        if triple_asterisk_count % 2 == 1 {
            return format!("{text}***");
        }
    }

    text.to_string()
}

/// Parses markdown text and completes incomplete tokens to prevent partial rendering.
pub fn parse_incomplete_markdown(text: &str) -> String {
    if text.is_empty() {
        return text.to_string();
    }

    // Handle incomplete links and images first.
    let mut result = handle_incomplete_links_and_images(text);

    // If a special incomplete link marker was added, don't process other formatting.
    if result.ends_with("](streamdown:incomplete-link)") {
        return result;
    }
    
    // The order of operations is important to handle nested and combined formatting.
    result = handle_incomplete_bold_italic(&result);
    result = handle_incomplete_bold(&result);
    result = handle_incomplete_double_underscore_italic(&result);
    result = handle_incomplete_single_asterisk_italic(&result);
    result = handle_incomplete_single_underscore_italic(&result);
    result = handle_incomplete_inline_code(&result);
    result = handle_incomplete_strikethrough(&result);
    result = handle_incomplete_block_katex(&result);

    result
}

// Unit tests to ensure the Rust implementation matches the TypeScript logic.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bold() {
        assert_eq!(handle_incomplete_bold("hello **world"), "hello **world**");
        assert_eq!(handle_incomplete_bold("hello **"), "hello **");
        assert_eq!(handle_incomplete_bold("hello **world**"), "hello **world**");
    }

    #[test]
    fn test_link_and_image() {
        assert_eq!(
            handle_incomplete_links_and_images("click [here"),
            "click [here](streamdown:incomplete-link)"
        );
        assert_eq!(handle_incomplete_links_and_images("see ![alt"), "see ");
    }
    
    #[test]
    fn test_strikethrough() {
        assert_eq!(handle_incomplete_strikethrough("~~strike"), "~~strike~~");
        assert_eq!(handle_incomplete_strikethrough("~~strike~~"), "~~strike~~");
    }

    #[test]
    fn test_inline_code() {
        assert_eq!(handle_incomplete_inline_code("`code"), "`code`");
        assert_eq!(handle_incomplete_inline_code("```rust\nfn main"), "```rust\nfn main");
    }

    #[test]
    fn test_katex_block() {
        assert_eq!(handle_incomplete_block_katex("$$math"), "$$math$$");
        assert_eq!(handle_incomplete_block_katex("$$x = y\n"), "$$x = y\n$$");
    }

    #[test]
    fn test_full_parse_flow() {
        let input = "This is **bold and `code";
        let expected = "This is **bold and `code`**";
        assert_eq!(parse_incomplete_markdown(input), expected);
    }
    
    #[test]
    fn test_single_underscore_in_word() {
        let input = "variable_name_is_long";
        assert_eq!(parse_incomplete_markdown(input), input);
    }

    #[test]
    fn test_single_underscore_as_italic() {
        let input = "this is _italic";
        let expected = "this is _italic_";
        assert_eq!(parse_incomplete_markdown(input), expected);
    }
    
    #[test]
    fn test_no_change_on_complete_markdown() {
        let input = "Here is some **valid** markdown with a [link](http://example.com) and `code`.";
        assert_eq!(parse_incomplete_markdown(input), input);
    }
}


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
    let modified_markdown_contenttemp = citation_regex.replace_all(markdown_input, |caps: &Captures| {
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

    let modified_markdown_content = parse_incomplete_markdown(&modified_markdown_contenttemp);




    log!("Markdown processing complete.");

    Ok(ProcessingOutput {
        modified_markdown: modified_markdown_content,
        bibliography_markdown: bibliography_content,
    })
}
