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

    exempt_blocks(&mut lines);
    let unwrapped_lines: Vec<OwnedLine> = unwrap_lines(lines);
    wrap_lines(unwrapped_lines, column_length)
}

fn open_exempt_tag(line: &str) -> &str {
    if line.contains("<!--") {
        return "<!--";
    }
    if line.contains("<pre") {
        return "<pre";
    }
    if line.contains("<xmp") {
        return "<xmp";
    }
    if line.contains("<style") {
        return "<style";
    }
    if line.contains("<script") {
        return "<script";
    }
    if line.contains("<svg") {
        return "<svg";
    }
    if line.contains("<table") {
        return "<table";
    }

    ""
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
        if in_exempt_block.len() == 0 {
            in_exempt_block = open_exempt_tag(&line.contents);
        }

        // If we're in an exempt block, mark the line as exempt from formatting,
        // and see if we've reached the close block.
        if in_exempt_block.len() > 0 {
            line.should_format = false;
            if contains_close_tag(in_exempt_block, &line.contents) {
                in_exempt_block = "";
            }
        }
    }
}

// Helpers.
lazy_static! {
    static ref SINGLE_TAG: Regex = Regex::new(r#"^</?[a-z-A-Z "=]+>$"#).unwrap();
    static ref FULL_DT_TAG: Regex = Regex::new(r#"<dt.*>.*</dt>$"#).unwrap();
}
fn is_standalone_line(line: &str) -> bool {
    line.len() == 0 || SINGLE_TAG.is_match(line) || FULL_DT_TAG.is_match(line)
}
fn must_break(line: &str) -> bool {
    line.ends_with("</li>") || line.ends_with("</dt>") || line.ends_with("</dd>")
}
fn exempt_from_wrapping(line: &str) -> bool {
    FULL_DT_TAG.is_match(line)
}

// TODO: This algorithm has a bug where if `git diff` describes an addition to a
// line in a perfectly-formatted paragraph, such that the addition makes the
// line now too long middle of a perfectly-formatted paragraph, we'll only
// rewrap that line, which might leave subsequent lines sub-optimally wrapped
// (too short). See https://github.com/domfarolino/specfmt/issues/8
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
            if previous_line_smushable == true && line.should_format {
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
        if line.contents.len() <= column_length.into()
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
    lazy_static! {
        static ref REGEX: Regex = Regex::new(r"^(\s*)").unwrap();
    }

    let mut return_lines = Vec::<String>::new();
    let indent = REGEX.captures(line).unwrap();
    let indent: &str = &indent[1];
    let line = line.trim_start();

    let mut words = line.split(" ");
    // This will never panic; even if `line` is empty after we trim it, the
    // split collection will contain a single empty string. See
    // https://play.rust-lang.org/?version=stable&mode=debug&edition=2021&gist=1035caa5a7a4324272c8966d36d323b4.
    let mut current_line = String::from(indent) + words.next().unwrap();
    for word in words {
        if current_line.len() + 1 + word.len() <= column_length.into() {
            current_line.push_str(&(" ".to_owned() + word));
        } else {
            if current_line != indent {
                return_lines.push(current_line);
            }
            current_line = String::from(indent);
            current_line.push_str(word);
        }
    }

    return_lines.push(current_line);
    return_lines
}
