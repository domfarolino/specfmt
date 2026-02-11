pub mod rewrapper;

// Re-export the main rewrapping function for convenience.
pub use rewrapper::rewrap_lines;

// A simple struct that we use to track each line of the source specification.
// When scoping our reformatting changes to lines in a `git diff`, lines in the
// spec do not also appear in the diff will have `should_format = false`. We
// dynamically make other lines exempt from formatting based on other exceptions
// and rules as well.
pub struct Line<'a> {
    pub should_format: bool,
    pub contents: &'a str,
}

// Parse git diff output to extract line numbers that were added/modified.
//
// This function implements a line-by-line parser that tracks the relationship between
// the git diff format and the actual line numbers in the source file being formatted.
//
// ## Algorithm Overview
//
// The git diff format uses `@@` lines to indicate line number context:
// ```
// @@ -old_start,old_count +new_start,new_count @@
// ```
//
// For example, `@@ -10,3 +10,5 @@` means:
// - Remove 3 lines starting at line 10 in the old file
// - Add 5 lines starting at line 10 in the new file
//
// ## Line Number Tracking Logic
//
// The parser maintains a `current_line_number` that represents the line number
// in the new file (the file we're formatting). This number is updated as we
// process each line in the diff:
//
// 1. **Header lines** (`+++`, `---`, `index`, `diff`): Skipped, no line number change
// 2. **@@ lines**: Set `current_line_number` to the `+new_start` value from the @@ line
// 3. **`+` lines** (additions):
//    - Add `current_line_number` to the result list of lines that need formatting (because
//      this content exists in the new file, *and* the git diff)
//    - Increment `current_line_number` (this line exists in the new file)
// 4. **`-` lines** (deletions):
//    - Don't add this line number to the result list of lines that need formatting (because this
//      content doesn't exist in the new file)
//    - Don't increment `current_line_number`
// 5. **Space lines** (unchanged context):
//    - Don't add this line number to the result list of lines that need formatting (because while
//      this content exists in the new file, it only appears in the git diff output as context, not
//      lines that were touched in the current branch)
//    - Increment `current_line_number` (this line exists in the new file)
//
// ## Example
//
// For a diff like:
// ```
// @@ -5,2 +5,3 @@
//  unchanged line
// -deleted line
// +added line 1
// +added line 2
// ```
//
// The parser would:
// - Start at line 5 (from `+5` in @@ line)
// - Skip the unchanged line, increment to line 6
// - Skip the deleted line, stay at line 6
// - Add line 6 to result, increment to line 7
// - Add line 7 to result, increment to line 8
//
// Result: `[6, 7]` (lines 6 and 7 in the source file that need formatting)
pub fn parse_diff_line_numbers(diff: &str, verbose: bool) -> Vec<usize> {
    let mut line_numbers = Vec::new();
    let mut current_line_number = 0;

    if verbose {
        eprintln!("DEBUG PARSING: Starting to parse diff with {} lines", diff.lines().count());
    }

    for (line_index, line) in diff.split('\n').enumerate() {
        // Skip header lines (don't increment line numbers)
        if line.starts_with("+++") || line.starts_with("---") || line.starts_with("index") || line.starts_with("diff") {
            if verbose {
                eprintln!("DEBUG PARSING: Skipping header line: '{}'", line);
            }
            continue;
        }

        // Parse @@ lines to get the line number context
        if line.starts_with("@@") {
            if verbose {
                eprintln!("DEBUG PARSING: Found @@ line {}: '{}'", line_index, line);
            }
            // Extract the line number from @@ -old_start,old_count +new_start,new_count @@
            if let Some(plus_part) = line.split("@@").nth(1) {
                if let Some(plus_section) = plus_part.split_whitespace().find(|s| s.starts_with('+')) {
                    if let Some(line_num_str) = plus_section.split(',').next() {
                        if let Ok(line_num) = line_num_str[1..].parse::<usize>() {
                            if verbose {
                                eprintln!("DEBUG PARSING: Parsed line number from @@: {} -> current_line_number = {}", line_num_str, line_num);
                            }
                            current_line_number = line_num;
                        }
                    }
                }
            }
        }
        // For lines starting with +, add the current line number
        else if line.starts_with('+') {
            if verbose {
                eprintln!("DEBUG PARSING: Found + line at current_line_number {}: '{}'", current_line_number, line);
                eprintln!("DEBUG PARSING: Added line {} to list, incrementing current_line_number from {} to {}", current_line_number, current_line_number, current_line_number + 1);
            }
            line_numbers.push(current_line_number);
            current_line_number += 1;
        }
        // For lines starting with -, don't increment (these are deletions from old file)
        else if line.starts_with('-') {
            if verbose {
                eprintln!("DEBUG PARSING: Found - line (deletion), NOT incrementing current_line_number: '{}'", line);
            }
        }
        // For lines starting with space, increment (these are unchanged lines in new file)
        // TODO(domfarolino): This should not be necessary, because the way this tool generates
        // the git diff does not include any unchanged context lines. This is only necessary
        // because the git_diff tests were generated with context lines. We should rebaseline
        // all of those tests and remove this condition.
        else if line.starts_with(' ') {
            if verbose {
                eprintln!("DEBUG PARSING: Found space line (unchanged), incrementing current_line_number from {} to {}", current_line_number, current_line_number + 1);
            }
            current_line_number += 1;
        }
    }

    if verbose {
        eprintln!("DEBUG PARSING: Final line_numbers list has {} entries", line_numbers.len());
    }

    line_numbers
}

// Marks specific lines in `lines` as needing format based on line numbers
// from the diff. This algorithm is precise because it uses line numbers
// instead of content matching, avoiding the duplicate line issue described
// in https://github.com/domfarolino/specfmt/issues/7.
pub fn apply_diff(lines: &mut Vec<Line>, diff_line_numbers: &Vec<usize>, verbose: bool) {
    if diff_line_numbers.is_empty() && verbose {
        println!("DEBUG: No lines to format");
        return;
    }

    if verbose {
        println!("DEBUG: Applying diff to {} lines, targeting line numbers: {:?}", lines.len(), diff_line_numbers);
    }

    for (i, line) in lines.iter_mut().enumerate() {
        let line_number = i + 1; // Convert to 1-based indexing
        if diff_line_numbers.contains(&line_number) {
            if verbose {
                println!("DEBUG: Marking line {} for formatting: '{}'", line_number, line.contents);
            }
            line.should_format = true;
        }
    }
}
