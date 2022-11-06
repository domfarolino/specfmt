use super::Line;
use lazy_static::lazy_static;
use regex::Regex;

pub fn rewrap_lines(lines: Vec<Line>, diff_lines: usize, column_length: u8) -> Vec<String> {
    println!("- - The Great Rewrapper - -");
    println!(
        "The spec has {} lines total. We'll try to wrap {} lines to {} characters",
        lines.len(),
        diff_lines,
        column_length
    );
    let unwrapped_lines: Vec<(bool, String)> = unwrap_lines(lines);
    wrap_lines(unwrapped_lines, column_length)
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
    line.ends_with("</li>") || line.ends_with("</dt>")
}
fn exempt_from_wrapping(line: &str) -> bool {
    FULL_DT_TAG.is_match(line)
}

fn unwrap_lines(lines: Vec<Line>) -> Vec<(bool, String)> {
    // TODO(domfarolino): We should be returning something like a `Vec<Line>`
    // here, but with owned strings (if necessary, as we currently have it)
    // instead of string slices. The tuple is a little opaque.
    let mut return_lines = Vec::<(bool, String)>::new();
    let mut previous_line_smushable = false;

    for line in lines {
        if is_standalone_line(line.contents.trim()) {
            return_lines.push((line.should_format, line.contents.to_string()));
            previous_line_smushable = false;
        } else {
            if previous_line_smushable == true && line.should_format {
                assert_ne!(return_lines.len(), 0);
                let n = return_lines.len();
                // If we're unwrapping this line by tacking it onto the end of
                // the previous one, we have to mark the previous line as a
                // candidate for formatting (it might not already be).
                return_lines[n - 1].0 = true;
                return_lines[n - 1]
                    .1
                    .push_str(&(String::from(" ") + line.contents.trim()));
            } else {
                return_lines.push((line.should_format, line.contents.to_string()));
            }

            previous_line_smushable = !must_break(line.contents);
        }
    }

    return_lines
}

fn wrap_lines(lines: Vec<(bool, String)>, column_length: u8) -> Vec<String> {
    let mut rewrapped_lines: Vec<String> = Vec::new();
    for (should_format, line) in lines.iter() {
        if line.len() <= column_length.into() || exempt_from_wrapping(line) || !should_format {
            rewrapped_lines.push(line.to_string());
        } else {
            rewrapped_lines.append(&mut wrap_single_line(&line, column_length));
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
