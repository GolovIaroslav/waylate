use std::process::{Command, Stdio};

pub fn read_primary_selection() -> Result<String, String> {
    read_text(true)
}

pub fn read_clipboard() -> Result<String, String> {
    read_text(false)
}

/// Normalizes multiline text (e.g. copied or selected from a PDF or wrapped terminal/viewer).
/// Replaces single line breaks with spaces, removes hyphenation at line breaks, collapses
/// multiple spaces, and preserves paragraph breaks (double newlines).
pub fn normalize_selected_text(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let normalized_crlf = trimmed.replace("\r\n", "\n").replace('\r', "\n");
    let raw_lines: Vec<&str> = normalized_crlf.lines().collect();

    // Some PDF viewers put a blank line between *every word* in the ordinary
    // clipboard, not only in PRIMARY. In that case blank lines cannot be
    // treated as paragraph boundaries. Only flatten strongly word-fragmented
    // text; preserve genuine paragraphs and lists for regular clipboard text.
    let non_empty: Vec<&str> = raw_lines
        .iter()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect();
    let looks_word_fragmented = raw_lines.len() > non_empty.len()
        && non_empty.len() >= 6
        && non_empty
            .iter()
            .filter(|line| line.split_whitespace().count() == 1)
            .count()
            * 5
            >= non_empty.len() * 4
        && non_empty
            .iter()
            .filter(|line| line.chars().next().is_some_and(char::is_lowercase))
            .count()
            >= 3
        && !non_empty
            .iter()
            .any(|line| line.starts_with('-') || line.starts_with('*') || line.starts_with('•'));
    if looks_word_fragmented {
        return normalize_primary_selection_text(&normalized_crlf);
    }

    let mut paragraphs: Vec<Vec<&str>> = Vec::new();
    let mut current_para: Vec<&str> = Vec::new();

    for line in raw_lines {
        let trimmed_line = line.trim();
        if trimmed_line.is_empty() {
            if !current_para.is_empty() {
                paragraphs.push(std::mem::take(&mut current_para));
            }
        } else {
            current_para.push(trimmed_line);
        }
    }
    if !current_para.is_empty() {
        paragraphs.push(current_para);
    }

    let mut cleaned_paragraphs: Vec<String> = Vec::new();

    for lines in paragraphs {
        let mut para_text = String::new();
        for (i, line) in lines.iter().enumerate() {
            if i == 0 {
                para_text.push_str(line);
                continue;
            }

            // Check if previous line ended with a hyphen connecting a word (e.g. "inter-" and "national")
            if para_text.ends_with('-') && !para_text.ends_with(" -") {
                let prev_char = para_text.chars().nth_back(1);
                let next_char = line.chars().next();
                if prev_char.map_or(false, |c| c.is_alphanumeric())
                    && next_char.map_or(false, |c| c.is_alphanumeric())
                {
                    para_text.pop();
                    para_text.push_str(line);
                    continue;
                }
            }

            para_text.push(' ');
            para_text.push_str(line);
        }

        // Collapse multiple whitespace within paragraph
        let mut collapsed = String::with_capacity(para_text.len());
        let mut last_was_space = false;
        for ch in para_text.chars() {
            if ch.is_whitespace() {
                if !last_was_space {
                    collapsed.push(' ');
                    last_was_space = true;
                }
            } else {
                collapsed.push(ch);
                last_was_space = false;
            }
        }
        let para_final = collapsed.trim().to_string();
        if !para_final.is_empty() {
            cleaned_paragraphs.push(para_final);
        }
    }

    cleaned_paragraphs.join("\n\n")
}

/// PRIMARY selection from PDF viewers may insert whitespace-only lines between words.
/// Unlike copied clipboard text, these are not reliable paragraph boundaries.
pub fn normalize_primary_selection_text(text: &str) -> String {
    let flattened = text.replace(['\r', '\u{2028}', '\u{2029}'], "\n");
    let lines: Vec<&str> = flattened
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let mut result = String::new();
    for line in lines {
        if result.ends_with('-')
            && !result.ends_with(" -")
            && result.chars().nth_back(1).is_some_and(char::is_alphabetic)
            && line.chars().next().is_some_and(char::is_alphabetic)
        {
            result.pop();
        } else if !result.is_empty() {
            result.push(' ');
        }
        result.push_str(line);
    }
    result.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn write_clipboard(text: &str) -> Result<(), String> {
    let mut child = Command::new("wl-copy")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|err| format!("Could not start wl-copy: {err}"))?;

    if let Some(stdin) = &mut child.stdin {
        use std::io::Write;
        stdin
            .write_all(text.as_bytes())
            .map_err(|err| format!("Could not send text to wl-copy: {err}"))?;
    }

    let status = child
        .wait()
        .map_err(|err| format!("Could not wait for wl-copy: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("wl-copy failed".into())
    }
}

fn read_text(primary: bool) -> Result<String, String> {
    let types = list_types(primary)?;
    let mime =
        preferred_text_mime(&types).ok_or_else(|| "Clipboard does not contain text".to_string())?;

    let mut args = Vec::new();
    if primary {
        args.push("--primary");
    }
    args.extend(["--no-newline", "--type", mime]);

    run_text_command("wl-paste", &args)
}

fn list_types(primary: bool) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    if primary {
        args.push("--primary");
    }
    args.push("--list-types");

    let raw = run_text_command("wl-paste", &args)?;
    Ok(raw.lines().map(|line| line.trim().to_string()).collect())
}

fn preferred_text_mime(types: &[String]) -> Option<&str> {
    for preferred in [
        "text/plain;charset=utf-8",
        "text/plain",
        "UTF8_STRING",
        "TEXT",
        "STRING",
    ] {
        if types.iter().any(|mime| mime == preferred) {
            return Some(preferred);
        }
    }
    types
        .iter()
        .find(|mime| mime.starts_with("text/"))
        .map(String::as_str)
}

fn run_text_command(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|err| format!("Could not start {program}: {err}"))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    String::from_utf8(output.stdout)
        .map(|text| text.trim().to_string())
        .map_err(|_| "Clipboard text is not valid UTF-8".to_string())
}

#[cfg(test)]
mod tests {
    use super::{normalize_primary_selection_text, normalize_selected_text, preferred_text_mime};

    #[test]
    fn rejects_image_only_clipboard() {
        let types = vec![
            "image/png".to_string(),
            "application/octet-stream".to_string(),
        ];
        assert_eq!(preferred_text_mime(&types), None);
    }

    #[test]
    fn prefers_plain_text_clipboard() {
        let types = vec![
            "image/png".to_string(),
            "text/plain;charset=utf-8".to_string(),
        ];
        assert_eq!(
            preferred_text_mime(&types),
            Some("text/plain;charset=utf-8")
        );
    }

    #[test]
    fn replaces_linebreaks_with_spaces() {
        let input = "This is a line in a\nPDF document with\r\nmultiple linebreaks.";
        assert_eq!(
            normalize_selected_text(input),
            "This is a line in a PDF document with multiple linebreaks."
        );
    }

    #[test]
    fn handles_hyphenated_linebreaks() {
        let input = "An inter-\nnational con-\nference.";
        assert_eq!(
            normalize_selected_text(input),
            "An international conference."
        );
    }

    #[test]
    fn handles_cyrillic_hyphenation_and_linebreaks() {
        let input = "Пример перено-\nса строки в\nдокументе.";
        assert_eq!(
            normalize_selected_text(input),
            "Пример переноса строки в документе."
        );
    }

    #[test]
    fn preserves_separate_paragraphs() {
        let input =
            "Paragraph 1 line 1\nParagraph 1 line 2\n\nParagraph 2 line 1\nParagraph 2 line 2";
        assert_eq!(
            normalize_selected_text(input),
            "Paragraph 1 line 1 Paragraph 1 line 2\n\nParagraph 2 line 1 Paragraph 2 line 2"
        );
    }

    #[test]
    fn collapses_redundant_spaces_and_tabs() {
        let input = "   Lots   of   \t  spaces  \n  and   tabs.   ";
        assert_eq!(normalize_selected_text(input), "Lots of spaces and tabs.");
    }

    #[test]
    fn preserves_dash_with_space() {
        let input = "Some concept - \nan explanation.";
        assert_eq!(
            normalize_selected_text(input),
            "Some concept - an explanation."
        );
    }

    #[test]
    fn pdf_primary_selection_has_no_word_per_line() {
        let input = "glaucoma\n \ncases\n \nthat\n \nwill\n \ncontinuously\n \nchallenge\n \nour\n \nre-\nsources\n \nworldwide";
        assert_eq!(
            normalize_primary_selection_text(input),
            "glaucoma cases that will continuously challenge our resources worldwide"
        );
    }

    #[test]
    fn pdf_word_per_line_with_blank_lines_from_user_report() {
        let input = "of\n \nthe\n\nNRR\n  \nare\n\ntypically\n\nexpected\n \nto\n\nsatisfy";
        assert_eq!(
            normalize_primary_selection_text(input),
            "of the NRR are typically expected to satisfy"
        );
    }

    #[test]
    fn pdf_clipboard_empty_lines_between_words_are_not_paragraphs() {
        let input = "of\n\nthe\n \nNRR\n\nare\n \ntypically\n\nexpected\n\nto\n\nsatisfy";
        assert_eq!(
            normalize_selected_text(input),
            "of the NRR are typically expected to satisfy"
        );
    }

    #[test]
    fn short_multiline_clipboard_paragraphs_stay_separate() {
        let input = "Introduction\n\nMethods\n\nResults";
        assert_eq!(
            normalize_selected_text(input),
            "Introduction\n\nMethods\n\nResults"
        );
    }

    #[test]
    fn pdf_unicode_separators_and_clipboard_paragraphs() {
        assert_eq!(
            normalize_primary_selection_text(
                "glaucoma\u{2028}\u{a0}\u{2028}cases\u{2029}re-\u{2028}sources"
            ),
            "glaucoma cases resources"
        );
        assert_eq!(
            normalize_selected_text("One\nline\n\nSecond\nline"),
            "One line\n\nSecond line"
        );
    }
}
