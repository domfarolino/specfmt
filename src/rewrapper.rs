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

fn open_exempt_tag(line: &str) -> &str {
    const EXEMPT_TAGS: [&str; 7] = [
        "<!--",
        "<pre",
        "<xmp",
        "<style",
        "<script",
        "<svg",
        "<table",
    ];

    EXEMPT_TAGS
        .iter()
        .min_by_key(|&&tag| line.find(tag).unwrap_or(usize::MAX))
        .filter(|&&tag| line.contains(tag))
        .copied()
        .unwrap_or("")
}

fn contains_close_tag(open_tag: &str, line: &str) -> bool {
    open_tag == "<!--" && line.contains("-->")
        || open_tag == "<pre" && line.contains("</pre>")
        || open_tag == "<xmp" && line.contains("</xmp>")
        || open_tag == "<style" && line.contains("</style>")
        || open_tag == "<script" && line.contains("</script>")
        || open_tag == "<svg" && line.contains("</svg>")
        || open_tag == "<table" && line.contains("</table>")
}

// This function exempts all of the lines appearing inside various blocks.
fn exempt_blocks(lines: &mut Vec<Line>) {
    let mut in_exempt_block: &str = "";
    for line in lines {
        // Only assign `in_exempt_block` if we're *not* already in one.
        if in_exempt_block.is_empty() {
            in_exempt_block = open_exempt_tag(line.contents);
        }

        // If we're in an exempt block, mark the line as exempt from formatting,
        // and see if we've reached the close block.
        if !in_exempt_block.is_empty() {
            line.should_format = false;
            if contains_close_tag(in_exempt_block, line.contents) {
                in_exempt_block = "";
            }
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
    static ref NUMBERED_LIST_ITEM: Regex = Regex::new(r"^\s*\d+\.\s").unwrap();
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

fn is_definition_term(line: &str) -> bool {
    DEFINITION_TERM.is_match(line)
}

fn is_definition_desc(line: &str) -> bool {
    DEFINITION_DESC.is_match(line)
}

// Add a new function to check if a line starts should start on a new line. This is kind of the inverse of
// `must_break()`; see the documentation above that function for more details.
fn must_start_on_new_line(line: &str) -> bool {
    is_definition_term(line) || is_definition_desc(line) || is_numbered_list_item(line)
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
        || is_numbered_list_item(line.trim_start())
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
            if lines[i].contents.trim().is_empty() || must_break(lines[i].contents ){
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

fn wrap_single_line(line: &str, column_length: u8) -> Vec<String> {
    let mut return_lines = Vec::<String>::new();
    let indent = line
        .chars()
        .take_while(|c| c.is_whitespace())
        .collect::<String>();

    let line = line.trim_start();

    // Calculate extra indentation. This may be computed by combining extra indentation from BOTH definition
    // description (3 spaces) *and* list indentation (2 spaces) if needed.
    let extra_indent = if is_definition_desc(line) {
        let desc_pos = line.find(":: ").map(|p| p + 3).unwrap_or(0);
        if is_numbered_list_item(&line[desc_pos..]) {
            // Add both the definition description indent and the numbered list indent
            let list_pos = line[desc_pos..].find(". ").map(|p| p + 2).unwrap_or(0);
            " ".repeat(desc_pos + list_pos)
        } else {
            " ".repeat(desc_pos)
        }
    } else if is_numbered_list_item(line) {
        let pos = line.find(". ").map(|p| p + 2).unwrap_or(0);
        " ".repeat(pos)
    } else if is_definition_term(line) {
        let pos = line.find(": ").map(|p| p + 2).unwrap_or(0);
        " ".repeat(pos)
    } else {
        String::new()
    };

    let mut words = line.split(' ');
    // This will never panic; even if `line` is empty after we trim it, the
    // split collection will contain a single empty string. See
    // https://play.rust-lang.org/?version=stable&mode=debug&edition=2021&gist=1035caa5a7a4324272c8966d36d323b4.
    let first_word = words.next().unwrap();
    let mut current_line = indent.clone() + first_word;

    for word in words {
        if current_line.chars().count() + 1 + word.chars().count() <= column_length.into() {
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
