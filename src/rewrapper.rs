use super::Line;
use lazy_static::lazy_static;
use regex::Regex;

// A struct similar to `Line`, with the exception that `OwnedLine` does not
// maintain a string reference, but rather an owned `String`. We cannot easily
// keep a reference to the original spec strings, because due to unwrapping,
// some of the lines of a spec have been mutated beyond the capability of
// slicing.
//
// That is, when turn `LINE + NEW_LINE + LINE2` into `LINE + SPACE + LINE2`, we
// are incapable of taking a slice over the entire line since it would include
// two non-contiguous slices separated by a brand new space character. We could
// modify `Line` to support this case where a given "line" consists of multiple
// string slices and owned string spaces, for efficiency, but for now we just use
// `OwnedLine` since it is easier.
pub struct OwnedLine {
    should_format: bool,
    contents: String,
}

pub fn rewrap_lines(mut lines: Vec<Line>, diff_lines: usize, column_length: u8) -> Vec<String> {
    println!("- - The Great Rewrapper - -");
    println!(
        "The spec has {} lines total. We'll try to wrap {} lines to {} characters",
        lines.len(),
        diff_lines,
        column_length
    );

    carryover_should_format_bit_where_necessary(&mut lines);
    exempt_dependencies_section(&mut lines);
    exempt_blocks(&mut lines);
    let unwrapped_lines: Vec<OwnedLine> = unwrap_lines(lines);
    wrap_lines(unwrapped_lines, column_length)
}

const EXEMPT_TAGS: [(&str, &str); 7] = [
    ("<!--", "-->"),
    ("<pre", "</pre>"),
    ("<xmp", "</xmp>"),
    ("<style", "</style>"),
    ("<script", "</script>"),
    ("<svg", "</svg>"),
    ("<table", "</table>"),
];

// Returns the earliest exempt open tag in `line`, along with the byte index
// just past it, or `None` if there is no such tag.
fn find_open_exempt_tag(line: &str) -> Option<(&'static str, usize)> {
    EXEMPT_TAGS
        .iter()
        .filter_map(|&(open, _)| line.find(open).map(|index| (open, index)))
        .min_by_key(|&(_, index)| index)
        .map(|(open, index)| (open, index + open.len()))
}

fn close_exempt_tag(open_tag: &str) -> &'static str {
    EXEMPT_TAGS
        .iter()
        .find(|&&(open, _)| open == open_tag)
        .map(|&(_, close)| close)
        .unwrap()
}

// This function exempts all of the lines appearing inside various blocks.
//
// Blocks are tracked positionally within a line, so that a block closing and
// another opening on the same line (e.g., `--><!--`) is handled correctly, and
// so that a close tag appearing *before* an open tag doesn't close the block
// that the open tag starts. Without this, prose inside a multi-line comment
// that merely mentions e.g. `<style>` would open a block that never closes.
fn exempt_blocks(lines: &mut Vec<Line>) {
    let mut in_exempt_block: &str = "";
    for line in lines {
        let mut exempt = !in_exempt_block.is_empty();
        let mut rest: &str = line.contents;
        loop {
            if in_exempt_block.is_empty() {
                match find_open_exempt_tag(rest) {
                    Some((open_tag, end)) => {
                        in_exempt_block = open_tag;
                        exempt = true;
                        rest = &rest[end..];
                    }
                    None => break,
                }
            }

            // We're in an exempt block; see if it closes later on this line.
            let close_tag = close_exempt_tag(in_exempt_block);
            match rest.find(close_tag) {
                Some(index) => {
                    in_exempt_block = "";
                    rest = &rest[index + close_tag.len()..];
                }
                None => break,
            }
        }

        if exempt {
            line.should_format = false;
        }
    }
}

fn exempt_dependencies_section(lines: &mut Vec<Line>) {
    let mut in_dependencies : bool = false;
    for line in lines {
        if in_dependencies {
            if line.contents.ends_with("</h4>") {
                return;
            }

            // Don't format the contents of new cross-specifications being added
            // to the dependencies section. These are added via new list items.
            if line.contents.contains("<li>") || line.contents.contains("<dfn") {
                line.should_format = false;
            }
        }

        if line.contents.ends_with("<h4>Dependencies</h4>") {
            in_dependencies = true;
            continue;
        }
    }
}

// Helpers.
lazy_static! {
    static ref SINGLE_TAG: Regex = Regex::new(r#"^</?[a-z-A-Z "=]+>$"#).unwrap();
    static ref FULL_DT_TAG: Regex = Regex::new(r#"<dt.*>.*</dt>$"#).unwrap();
    static ref HEADER_TAG: Regex = Regex::new(r#"<h[0-6].*>.*</h[0-6]>$"#).unwrap();
    static ref NUMBERED_LIST_ITEM: Regex = Regex::new(r"[0-9]+[.]?[0-9]*\.\s").unwrap();
    static ref UNORDERED_LIST_ITEM: Regex = Regex::new(r"^\s*[\*-]\s").unwrap();
    static ref DEFINITION_TERM: Regex = Regex::new(r"^\s*:\s").unwrap();
    static ref DEFINITION_DESC: Regex = Regex::new(r"^\s*::\s").unwrap();
}

fn is_standalone_line(line: &str) -> bool {
    line.is_empty()
        || SINGLE_TAG.is_match(line)
        || FULL_DT_TAG.is_match(line)
        || HEADER_TAG.is_match(line)
}

fn is_numbered_list_item(line: &str) -> bool {
    NUMBERED_LIST_ITEM.is_match(line)
}

fn is_unordered_list_item(line: &str) -> bool {
    UNORDERED_LIST_ITEM.is_match(line)
}

fn is_definition_term(line: &str) -> bool {
    DEFINITION_TERM.is_match(line)
}

fn is_definition_desc(line: &str) -> bool {
    DEFINITION_DESC.is_match(line)
}

// Add a new function to check if a line starts should start on a new line. This
// is kind of the inverse of `must_break()`; see the documentation above that
// function for more details.
fn must_start_on_new_line(line: &str) -> bool {
    is_definition_term(line) || is_definition_desc(line) || is_numbered_list_item(line) || is_unordered_list_item(line)
}

// This differs from `is_standalone_line()` in that it is a weaker check. If
// `is_standalone_line()` is true, then we prevent:
//   (a): The current line from being appended to the end of earlier lines
//   (b): Later lines from being appended to the end of the current line
// If a given line isn't "standalone", it can be appended to a previous line,
// but if `must_break()` is true, we prevent later lines from being appended to
// the end of the current line. So `must_break()` is a strictly less-powerful
// condition to gate behavior on.
fn must_break(line: &str) -> bool {
    line.ends_with("</li>")
        || line.ends_with("</p>")
        || line.ends_with("</dt>")
        || line.ends_with("</dd>")
        || line.ends_with("-->")
        || is_definition_term(line.trim_start())
}

fn exempt_from_wrapping(line: &str) -> bool {
    FULL_DT_TAG.is_match(line)
}

// Ensure that when a single line in the middle of a group of lines is marked as
// `should_format`, the bit is carried down to all subsequent lines until
// necessary.
fn carryover_should_format_bit_where_necessary(lines: &mut Vec<Line>) {
    let mut should_format_current_line = false;

    for i in 0..lines.len() {
        if lines[i].should_format {
            should_format_current_line = true;
        }

        // This is either true because of the line immediately above, or because
        // we're carrying it over from a previous line. We use it to mark all
        // subsequent lines as `should_format` until we hit a terminating
        // condition that tells us to stop.
        if should_format_current_line {
            // If we get here, then `lines[i]` does not have `should_format`
            // explicitly true (because it was not directly modified), but it
            // follows an explicitly `should_format` line. Therefore, we have to
            // format the line anyways...
            lines[i].should_format = true;

            // But we have to stop carrying on this "implicit format" trend once
            // we hit a line that meets an "implicit format terminating".
            //
            // TODO(domfarolino): Consider using `must_break` below instead of
            // the specific end-p condition.
            if lines[i].contents.trim().is_empty() || must_break(lines[i].contents) {
                should_format_current_line = false;
            }
        }
    }
}

fn unwrap_lines(lines: Vec<Line>) -> Vec<OwnedLine> {
    let mut return_lines = Vec::<OwnedLine>::new();
    let mut previous_line_smushable = false;

    for line in lines {
        if is_standalone_line(line.contents.trim()) {
            return_lines.push(OwnedLine {
                should_format: line.should_format,
                contents: line.contents.to_string(),
            });
            previous_line_smushable = false;
        } else {
            if previous_line_smushable && line.should_format && !must_start_on_new_line(line.contents.trim()) {
                assert_ne!(return_lines.len(), 0);
                let n = return_lines.len();
                // If we're unwrapping this line by tacking it onto the end of
                // the previous one, we have to mark the previous line as a
                // candidate for formatting (it might not already be).
                return_lines[n - 1].should_format = true;
                return_lines[n - 1]
                    .contents
                    .push_str(&(String::from(" ") + line.contents.trim()));
            } else {
                return_lines.push(OwnedLine {
                    should_format: line.should_format,
                    contents: line.contents.to_string(),
                });
            }
            previous_line_smushable = !must_break(line.contents);
        }
    }

    return_lines
}

fn wrap_lines(lines: Vec<OwnedLine>, column_length: u8) -> Vec<String> {
    let mut rewrapped_lines: Vec<String> = Vec::new();
    for line in lines.iter() {
        if line.contents.chars().count() <= column_length.into()
            || exempt_from_wrapping(&line.contents)
            || !line.should_format
        {
            rewrapped_lines.push(line.contents.to_string());
        } else {
            rewrapped_lines.append(&mut wrap_single_line(&line.contents, column_length));
        }
    }

    rewrapped_lines
}

// Tags whose start tag is never broken across lines, since wrapping in the
// middle of one makes the source much harder to read.
const UNSPLITTABLE_TAGS: [&str; 2] = ["img", "iframe"];

// Finds the `[start, end]` byte ranges of every start tag in `line` whose tag
// name is in `UNSPLITTABLE_TAGS`, where `end` is the index of the tag's closing
// `>`. Quoted attribute values are tracked, so a `>` inside an attribute value
// doesn't end the tag early.
fn unsplittable_tag_ranges(line: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::<(usize, usize)>::new();
    let bytes = line.as_bytes();
    let mut search_start = 0;

    while let Some(offset) = line[search_start..].find('<') {
        let tag_start = search_start + offset;
        let after_name = UNSPLITTABLE_TAGS.iter().find_map(|tag| {
            let after_name = tag_start + 1 + tag.len();
            // Only an exact tag name counts; this skips over things like
            // `<image` or `<imgfoo`.
            if line[tag_start + 1..].starts_with(tag)
                && matches!(bytes.get(after_name), Some(b' ') | Some(b'\t') | Some(b'/') | Some(b'>'))
            {
                Some(after_name)
            } else {
                None
            }
        });

        let after_name = match after_name {
            Some(after_name) => after_name,
            None => {
                search_start = tag_start + 1;
                continue;
            }
        };

        let mut quote: Option<u8> = None;
        let mut tag_end: Option<usize> = None;
        for (i, &byte) in bytes.iter().enumerate().skip(after_name) {
            match byte {
                b'"' | b'\'' => {
                    if quote == Some(byte) {
                        quote = None;
                    } else if quote.is_none() {
                        quote = Some(byte);
                    }
                }
                b'>' if quote.is_none() => {
                    tag_end = Some(i);
                    break;
                }
                _ => {}
            }
        }

        match tag_end {
            Some(tag_end) => {
                ranges.push((tag_start, tag_end));
                search_start = tag_end + 1;
            }
            // An unterminated tag; there is nothing left on this line that we
            // could keep together anyways.
            None => break,
        }
    }

    ranges
}

// Splits `line` on spaces, the same way `str::split(' ')` would, except that
// the start tags of `UNSPLITTABLE_TAGS` elements are never split.
fn split_into_words(line: &str) -> Vec<&str> {
    let tag_ranges = unsplittable_tag_ranges(line);
    if tag_ranges.is_empty() {
        return line.split(' ').collect();
    }

    let mut words = Vec::<&str>::new();
    let mut word_start = 0;
    for (i, character) in line.char_indices() {
        if character != ' ' {
            continue;
        }
        // Spaces inside an unsplittable tag are not word boundaries.
        if tag_ranges
            .iter()
            .any(|&(tag_start, tag_end)| i > tag_start && i < tag_end)
        {
            continue;
        }
        words.push(&line[word_start..i]);
        word_start = i + 1;
    }
    words.push(&line[word_start..]);

    words
}

fn wrap_single_line(line: &str, column_length: u8) -> Vec<String> {
    let mut return_lines = Vec::<String>::new();
    let indent = line
        .chars()
        .take_while(|c| c.is_whitespace())
        .collect::<String>();

    let line = line.trim_start();

    // Calculate extra indentation. This may be computed by combining extra
    // indentation from BOTH definition description (3 spaces) *and* list
    // indentation (2 spaces) if needed.
    let extra_indent = if is_definition_desc(line) {
        let desc_pos = line.find(":: ").map(|p| p + 3).unwrap_or(0);
        if is_numbered_list_item(&line[desc_pos..]) || is_unordered_list_item(&line[desc_pos..]) {
            // Add both the definition description indent and the list indent
            let list_pos = if is_numbered_list_item(&line[desc_pos..]) {
                line[desc_pos..].find(". ").map(|p| p + 2)
            } else {
                // Look for either "* " or "- ".
                line[desc_pos..].find("* ")
                    .or_else(|| line[desc_pos..].find("- "))
                    .map(|p| p + 2)
            }.unwrap_or(0);
            " ".repeat(desc_pos + list_pos)
        } else {
            " ".repeat(desc_pos)
        }
    } else if is_numbered_list_item(line) {
        let pos = line.find(". ").map(|p| p + 2).unwrap_or(0);
        " ".repeat(pos)
    } else if is_unordered_list_item(line) {
        // Find position of either "* " or "- ".
        let pos = line.find("* ")
            .or_else(|| line.find("- "))
            .map(|p| p + 2)
            .unwrap_or(0);
        " ".repeat(pos)
    } else if is_definition_term(line) {
        let pos = line.find(": ").map(|p| p + 2).unwrap_or(0);
        " ".repeat(pos)
    } else {
        String::new()
    };

    let mut words = split_into_words(line).into_iter();
    // This will never panic; even if `line` is empty after we trim it, the
    // split collection will contain a single empty string. See
    // https://play.rust-lang.org/?version=stable&mode=debug&edition=2021&gist=1035caa5a7a4324272c8966d36d323b4.
    let first_word = words.next().unwrap();
    let mut current_line = indent.clone() + first_word;

    for word in words {
        // A word that is over-long only because it swallowed an
        // unsplittable tag overflows whatever line it lands on, and nothing
        // can follow it on that line either way, so breaking before it buys
        // nothing and just leaves a stub such as `<p` behind. Words that are
        // over-long on their own, such as a long URL, keep breaking as before.
        let word_never_fits = word.chars().count() > column_length.into()
            && !unsplittable_tag_ranges(word).is_empty();

        if current_line.chars().count() + 1 + word.chars().count() <= column_length.into()
            || word_never_fits
        {
            current_line.push_str(&(" ".to_owned() + word));
        } else {
            if current_line != indent {
                return_lines.push(current_line);
            }
            current_line = indent.clone() + &extra_indent + word;
        }
    }

    return_lines.push(current_line);
    return_lines
}
