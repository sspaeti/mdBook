//! Frontmatter parsing support for mdBook.
//!
//! Extracts YAML frontmatter from markdown content and injects
//! Open Graph / Twitter Card metadata and date information
//! into the Handlebars template context.

use serde::Deserialize;
use serde_json::json;

/// Parsed YAML frontmatter fields.
/// All fields are optional so chapters can have any subset of metadata.
#[derive(Deserialize, Debug, Default)]
#[serde(default)]
pub(crate) struct FrontMatter {
    /// Page title for OG/Twitter meta tags.
    pub title: Option<String>,
    /// Page description for OG/Twitter meta tags.
    pub description: Option<String>,
    /// Featured image URL for OG/Twitter meta tags.
    pub featured_image_url: Option<String>,
    /// Creation date in YYYY-MM-DD format.
    pub createddate: Option<String>,
    /// Last modified date in YYYY-MM-DD format.
    pub lastmod: Option<String>,
}

/// Formats a "YYYY-MM-DD" date string into human-readable "Mon DD, YYYY" format.
/// Returns None if the date string is malformed.
fn format_display_date(date_str: &str) -> Option<String> {
    let parts: Vec<&str> = date_str.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let year = parts[0];
    let month: u32 = parts[1].parse().ok()?;
    let day: u32 = parts[2].parse().ok()?;

    let month_abbr = match month {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => return None,
    };

    Some(format!("{} {}, {}", month_abbr, day, year))
}

/// Strips YAML frontmatter (between `---` markers) from content,
/// returning the content without the frontmatter block.
pub(crate) fn strip_frontmatter(content: &str) -> String {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return content.to_string();
    }
    // Find the closing `---` after the opening one
    let after_open = &trimmed[3..];
    if let Some(end) = after_open.find("\n---") {
        // Skip past the closing `---` and any trailing newline
        let rest = &after_open[end + 4..];
        rest.trim_start_matches('\n').to_string()
    } else {
        content.to_string()
    }
}

/// Counts words in markdown content, stripping frontmatter, code blocks,
/// HTML tags, URLs, and markdown syntax to match Obsidian's word count.
fn count_words(content: &str) -> usize {
    let stripped = strip_frontmatter(content);
    let mut result = String::with_capacity(stripped.len());
    let mut in_code_block = false;

    for line in stripped.lines() {
        let trimmed = line.trim();

        // Toggle fenced code blocks (``` or ~~~)
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code_block = !in_code_block;
            continue;
        }
        if in_code_block {
            continue;
        }

        // Skip admonish/callout block markers
        if trimmed.starts_with("```admonish") {
            continue;
        }

        // Skip HTML tags (standalone lines like <div>, <!-- -->, etc.)
        if trimmed.starts_with('<') && trimmed.ends_with('>') {
            continue;
        }

        result.push(' ');
        result.push_str(line);
    }

    // Remove inline markdown syntax:
    // - URLs from [text](url) -> keep "text"
    // - Image syntax ![alt](url) -> skip entirely
    // - HTML tags <...>
    // - Bold/italic markers
    let mut clean = result.clone();

    // Remove images ![alt](url)
    while let Some(start) = clean.find("![") {
        if let Some(paren_start) = clean[start..].find("](") {
            if let Some(paren_end) = clean[start + paren_start..].find(')') {
                clean.replace_range(start..start + paren_start + paren_end + 1, "");
                continue;
            }
        }
        break;
    }

    // Replace [text](url) with just "text"
    let mut out = String::with_capacity(clean.len());
    let mut chars = clean.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '[' {
            // Collect link text
            let mut link_text = String::new();
            let mut found_close = false;
            for ch in chars.by_ref() {
                if ch == ']' {
                    found_close = true;
                    break;
                }
                link_text.push(ch);
            }
            if found_close && chars.peek() == Some(&'(') {
                chars.next(); // skip '('
                // Skip URL until ')'
                for ch in chars.by_ref() {
                    if ch == ')' {
                        break;
                    }
                }
                out.push_str(&link_text);
            } else {
                out.push('[');
                out.push_str(&link_text);
                if found_close {
                    out.push(']');
                }
            }
        } else {
            out.push(c);
        }
    }

    // Remove HTML tags
    let mut no_html = String::with_capacity(out.len());
    let mut in_tag = false;
    for c in out.chars() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
            no_html.push(' ');
        } else if !in_tag {
            no_html.push(c);
        }
    }

    no_html.split_whitespace()
        .filter(|word| {
            // Skip pure markdown syntax tokens
            if matches!(*word, "#" | "##" | "###" | "####" | "#####" | "######"
                | "---" | "***" | "___" | "-" | "*" | ">" | "|" | "```") {
                return false;
            }
            // Skip words that are just punctuation/symbols
            if word.chars().all(|c| matches!(c, '*' | '_' | '~' | '`' | '|' | '-')) {
                return false;
            }
            true
        })
        .count()
}

/// Parses YAML frontmatter from content and injects metadata
/// into the Handlebars template context data map.
/// When `enable_word_count` is true, also calculates word count and
/// reading time (at 212 wpm, matching Hugo).
pub(crate) fn inject_frontmatter_data(
    content: &str,
    data: &mut serde_json::Map<String, serde_json::Value>,
    enable_word_count: bool,
) {
    // Inject word count and reading time if enabled
    if enable_word_count {
        let word_count = count_words(content);
        let reading_time = (word_count as f64 / 212.0).ceil() as usize;
        data.insert("word_count".to_owned(), json!(word_count));
        data.insert("reading_time".to_owned(), json!(reading_time));
    }

    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return;
    }
    let after_open = &trimmed[3..];
    let Some(end) = after_open.find("\n---") else {
        return;
    };
    let yaml_str = &after_open[..end];

    match serde_yml::from_str::<FrontMatter>(yaml_str) {
        Ok(fm) => {
            // OG metadata — only set is_frontmatter when OG fields are present
            let has_og = fm.title.is_some()
                || fm.description.is_some()
                || fm.featured_image_url.is_some();
            if has_og {
                data.insert("is_frontmatter".to_owned(), json!(true));
            }
            if let Some(ref title) = fm.title {
                data.insert("og_title".to_owned(), json!(title));
            }
            if let Some(ref desc) = fm.description {
                data.insert("og_description".to_owned(), json!(desc));
            }
            if let Some(ref img) = fm.featured_image_url {
                data.insert("og_image_url".to_owned(), json!(img));
            }

            // Date metadata
            let has_dates = fm.createddate.is_some() || fm.lastmod.is_some();
            if has_dates {
                data.insert("has_dates".to_owned(), json!(true));
            }
            if let Some(ref date) = fm.createddate {
                data.insert("createddate".to_owned(), json!(date));
                if let Some(display) = format_display_date(date) {
                    data.insert("createddate_display".to_owned(), json!(display));
                }
            }
            if let Some(ref date) = fm.lastmod {
                data.insert("lastmod".to_owned(), json!(date));
                if let Some(display) = format_display_date(date) {
                    data.insert("lastmod_display".to_owned(), json!(display));
                }
            }
        }
        Err(e) => {
            eprintln!("Frontmatter: deserialization error: {e:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_frontmatter() {
        let input = "---\ntitle: \"Hello\"\n---\n# Content";
        assert_eq!(strip_frontmatter(input), "# Content");
    }

    #[test]
    fn test_strip_no_frontmatter() {
        let input = "# Just content";
        assert_eq!(strip_frontmatter(input), "# Just content");
    }

    #[test]
    fn test_inject_frontmatter_data() {
        let input = "---\ntitle: \"My Title\"\ndescription: \"My Desc\"\nfeatured_image_url: \"https://example.com/img.png\"\n---\n# Content";
        let mut data = serde_json::Map::new();
        inject_frontmatter_data(input, &mut data, true);
        assert_eq!(data["is_frontmatter"], json!(true));
        assert_eq!(data["og_title"], json!("My Title"));
        assert_eq!(data["og_description"], json!("My Desc"));
        assert_eq!(data["og_image_url"], json!("https://example.com/img.png"));
    }

    #[test]
    fn test_inject_no_frontmatter() {
        let input = "# Just content";
        let mut data = serde_json::Map::new();
        inject_frontmatter_data(input, &mut data, true);
        assert!(!data.contains_key("is_frontmatter"));
    }

    #[test]
    fn test_inject_dates_only() {
        let input = "---\ncreateddate: \"2024-03-15\"\nlastmod: \"2025-09-12\"\n---\n# Content";
        let mut data = serde_json::Map::new();
        inject_frontmatter_data(input, &mut data, true);
        // No OG fields -> is_frontmatter should NOT be set
        assert!(!data.contains_key("is_frontmatter"));
        // Dates should be set
        assert_eq!(data["has_dates"], json!(true));
        assert_eq!(data["createddate"], json!("2024-03-15"));
        assert_eq!(data["lastmod"], json!("2025-09-12"));
        assert_eq!(data["createddate_display"], json!("Mar 15, 2024"));
        assert_eq!(data["lastmod_display"], json!("Sep 12, 2025"));
    }

    #[test]
    fn test_inject_full_frontmatter_with_dates() {
        let input = "---\ntitle: \"My Title\"\ndescription: \"Desc\"\nfeatured_image_url: \"https://img.png\"\ncreateddate: \"2023-01-01\"\nlastmod: \"2025-12-25\"\n---\n# Content";
        let mut data = serde_json::Map::new();
        inject_frontmatter_data(input, &mut data, true);
        assert_eq!(data["is_frontmatter"], json!(true));
        assert_eq!(data["og_title"], json!("My Title"));
        assert_eq!(data["has_dates"], json!(true));
        assert_eq!(data["createddate_display"], json!("Jan 1, 2023"));
        assert_eq!(data["lastmod_display"], json!("Dec 25, 2025"));
    }

    #[test]
    fn test_word_count_and_reading_time() {
        let input = "---\ntitle: \"Test\"\n---\n# Hello\n\nThis is a simple paragraph with some words in it.";
        let mut data = serde_json::Map::new();
        inject_frontmatter_data(input, &mut data, true);
        // "Hello This is a simple paragraph with some words in it." = 11 words (# is filtered)
        assert_eq!(data["word_count"], json!(11));
        assert_eq!(data["reading_time"], json!(1)); // ceil(11/212) = 1
    }

    #[test]
    fn test_word_count_no_frontmatter() {
        let input = "# Just content\n\nSome words here.";
        let mut data = serde_json::Map::new();
        inject_frontmatter_data(input, &mut data, true);
        // "Just content Some words here." = 5 words (# is filtered)
        assert_eq!(data["word_count"], json!(5));
        assert_eq!(data["reading_time"], json!(1));
    }

    #[test]
    fn test_word_count_skips_code_blocks_and_urls() {
        let input = "# Title\n\nSome text with a [link](https://example.com) here.\n\n```python\ndef foo():\n    return 42\n```\n\nMore words after code.";
        // Should count: Title Some text with a link here. More words after code. = 11
        // Should NOT count: code block contents, URL
        assert_eq!(count_words(input), 11);
    }

    #[test]
    fn test_format_display_date() {
        assert_eq!(format_display_date("2025-09-12"), Some("Sep 12, 2025".to_string()));
        assert_eq!(format_display_date("2023-01-01"), Some("Jan 1, 2023".to_string()));
        assert_eq!(format_display_date("2024-12-25"), Some("Dec 25, 2024".to_string()));
        assert_eq!(format_display_date("invalid"), None);
        assert_eq!(format_display_date("2025-13-01"), None);
    }
}
