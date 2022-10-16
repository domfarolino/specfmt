use lazy_static::lazy_static;
use regex::Regex;

// TODO: Implement unwrapping, to run before wrapping.

pub fn wrap_lines(lines: Vec<&str>, column_length: u8) -> Vec<String> {
    println!("- - The Great Rewrapper - -");
    println!(
        "We're dealing with {} lines total, and wrapping to {} characters",
        lines.len(),
        column_length
    );

    let mut rewrapped_lines: Vec<String> = Vec::new();
    for line in lines.iter() {
        if line.len() <= column_length.into() {
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
