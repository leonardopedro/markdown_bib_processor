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


    


    result = handle_incomplete_inline_code(&result);

    result = handle_incomplete_bold_italic(&result);

    result = handle_incomplete_bold(&result);

    result = handle_incomplete_double_underscore_italic(&result);

    result = handle_incomplete_single_asterisk_italic(&result);

    result = handle_incomplete_single_underscore_italic(&result);

    result = handle_incomplete_strikethrough(&result);

    result = handle_incomplete_block_katex(&result);



    result

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
        let author_part = cap.get(2).map_or("", |m| m.as_str()).to_string();
        let year_part = cap.get(3).map_or("", |m| m.as_str()).to_string();
        let suffix_part = cap.get(4).map_or("", |m| m.as_str()).to_string();
        let full_match = author_part.clone()+&year_part+if suffix_part=="a"{""} else {&suffix_part};
        
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
            let mut best_fuzzy_match: Option<(usize, (&(String, String), &Vec<&Entry>))> = None;

            for entry in &grouped_entries {
                    if entry.0.1== *year_part {
                            let distance = levenshtein(&md_author_lc, &entry.0.0);
                            if distance <= FUZZY_MATCH_THRESHOLD
                                && (best_fuzzy_match.is_none()
                                    || distance < best_fuzzy_match.as_ref().unwrap().0)
                            {
                                best_fuzzy_match = Some((distance, entry));
                            }
                        }
            }
         
            if let Some((_dist, selected_entry)) = best_fuzzy_match {
                let index = suffix_to_index(suffix_part);
                if let Some(selected_entry2) = selected_entry.1.get(index) {
                  final_entry_map.insert(author_part.clone()+year_part+(if suffix_part=="a"{""} else {suffix_part}), selected_entry2);
                  missing_keys.remove(md_key);
                }
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
    let mut citation_indices: HashMap<String, (usize, String)> = HashMap::new();
    for (i, entry) in bibliography_items_to_render.iter().enumerate() {
        citation_indices.insert(entry.0.key().to_string(), (i + 1,entry.1.to_string()));
    }

    let modified_markdown_content = parse_incomplete_markdown(&citation_regex
        .replace_all(markdown_input, |caps: &Captures| {
           //let full_match = caps.get(1).map_or("", |m| m.as_str()).to_string();
           let author_part = caps.get(2).map_or("", |m| m.as_str()).to_string();
           let year_part = caps.get(3).map_or("", |m| m.as_str()).to_string();
           let suffix_part = caps.get(4).map_or("", |m| m.as_str()).to_string();
           let anchor = author_part.clone()+&year_part+(if suffix_part=="a"{""} else {&suffix_part});

            if let Some(entry) = final_entry_map.get(&anchor) {
                if let Some((_index,anch)) = citation_indices.get(entry.key()) {
                    let link = format!("[[{}]]({}#{})", anch, bibliography_link_prefix, anch);
                    return link;
                }
            }
            ["@", &anchor].join("")
        })
        .to_string());

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