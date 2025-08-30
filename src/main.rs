use clap::CommandFactory;
use clap::Parser;
use std::fs::read_dir;
use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

// Adapted from the web version of the original rewrapper
// (https://github.com/domenic/rewrapper).

mod rewrapper;

// A simple struct that we use to track each line of the source specification.
// When scoping our reformatting changes to lines in a `git diff`, lines in the
// spec do not also appear in the diff will have `should_format = false`. We
// dynamically make other lines exempt from formatting based on other exceptions
// and rules as well.
pub struct Line<'a> {
    should_format: bool,
    contents: &'a str,
}

fn read_file(filename: &Path) -> Result<(File, String), io::Error> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .append(false)
        .open(filename)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok((file, contents))
}

fn write_file(mut file: File, contents: String) -> Result<u8, io::Error> {
    // Will always work because `file` is opened for writing.
    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(contents.as_bytes())?;
    Ok(0)
}

/// Formats Bikeshed and Wattsi specifications using WHATWG conventions.
#[derive(Parser, Debug)]
#[command(version)]
struct Args {
    /// The specification to reformat. Defaults to "source" or the unique .bs
    /// file in the current directory.
    filename: Option<String>,

    /// Number of columns to wrap to.
    #[arg(long, default_value_t = 100)]
    wrap: u8,

    /// Force-reformat the spec even if it has uncommitted changes.
    #[arg(short, long, default_value_t = false)]
    force: bool,

    /// Reformat the entire spec, not scoped to the changes of the current branch.
    #[arg(long, default_value_t = false)]
    full_spec: bool,

    /// Base branch to compare the current branch with.
    #[arg(long)]
    base_branch: Option<String>,

    /// Enable verbose debugging output for troubleshooting git diff parsing.
    #[arg(long, default_value_t = false)]
    verbose: bool,
}

fn default_filename(filename: Option<String>) -> Result<PathBuf, clap::error::Error> {
    let mut directory = String::from(".");
    if let Some(filename) = filename {
        let path = PathBuf::from(filename);
        // If you pass in a file, we simply use it.
        if path.is_file() {
            return Ok(path);
        }

        // If you pass in something else (a valid directory, or something that
        // does not exist), then we'll use that that as the base for our search
        // for the appropriate spec file.
        directory = String::from(path.to_str().unwrap());
    }

    let source_path = directory.clone() + "/source";
    if Path::new(&source_path).exists() {
        return Ok(PathBuf::from(&source_path));
    }
    if let Ok(entries) = read_dir(directory) {
        let bs_files: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                if let Some(ext) = path.extension() {
                    return ext == "bs";
                }
                false
            })
            .collect();
        if bs_files.len() == 1 {
            return Ok(bs_files[0].clone());
        }
        if bs_files.len() > 1 {
            return Err(Args::command().error(
                clap::error::ErrorKind::MissingRequiredArgument,
                "Must specify filename: directory contains multiple .bs files",
            ));
        }
    }
    Err(Args::command().error(
        clap::error::ErrorKind::MissingRequiredArgument,
        "Must specify filename: directory doesn't contain \"source\" or .bs spec",
    ))
}

fn assert_no_uncommitted_changes(path: &Path) -> Result<(), clap::error::Error> {
    // Extract the filename itself, as well as the directory from `path`.
    assert!(path.is_file());
    let filename_without_path = path.file_name().unwrap();
    let directory = path.parent().unwrap();

    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(directory)
        .arg("status")
        .arg("--porcelain")
        .arg(filename_without_path)
        .output()
        .expect("Failed to run `git status");

    // This means that the spec we're targeting does not have uncommitted
    // changes, so we're safe to proceed with rewrapping.
    if output.stdout.is_empty() {
        return Ok(());
    }
    Err(Args::command().error(
        clap::error::ErrorKind::ValueValidation,
        "Spec has uncommitted changes. Please commit your changes and try again.",
    ))
}

// If there are no errors, this returns the computed diff of the target spec's
// current branch and base branch (master or main). The output should be
// filtered by `sanitized_diff_lines()`.
fn git_diff(path: &Path, base_branch_opt: Option<String>) -> Result<String, clap::error::Error> {
    // Extract the filename itself, as well as the directory from `path`.
    assert!(path.is_file());
    let filename_without_path = path.file_name().unwrap().to_str().unwrap();
    let directory = path.parent().unwrap().to_str().unwrap();

    // Get the name of the git branch that the spec is currently on.
    let current_branch = std::process::Command::new("git")
        .arg("-C")
        .arg(directory)
        .arg("branch")
        .arg("--show-current")
        .output()
        .expect("Failed to run `git branch --show-current`");
    let current_branch = String::from_utf8(current_branch.stdout).unwrap();
    let current_branch = current_branch.trim();

    let base_branch = if let Some(branch) = base_branch_opt {
        branch
    } else {
        // Get the base branch to compare `current_branch` to with in `git diff`. We
        // expect it to be either `master` or `main`, and fail otherwise.
        let branches = std::process::Command::new("git")
            .arg("-C")
            .arg(directory)
            .arg("for-each-ref")
            .arg("--format=%(refname:short)")
            .output()
            .expect("Failed to find the base branch to compare current branch '${}' with");
        let branches = String::from_utf8(branches.stdout).unwrap();
        let branches = branches.split('\n');

        let mut computed_base = String::new();
        for branch in branches {
            if branch == "origin/main" {
                computed_base = branch.to_string();
                break;
            }
            // Prioritize "main" derivatives over "master", but don't stop looking
            // for "origin/main". That seems to be needed in most forks.
            if branch == "origin/main" || branch == "main" {
                computed_base = branch.to_string();
            }
            // Only use derivatives of "master" if we haven't selected anything else.
            if branch == "origin/master" || branch == "master" && computed_base.is_empty() {
                // If we found a "master" derivative, then hold onto it for now, but
                // keep looking in case we find a "main" one later.
                computed_base = branch.to_string();
            }
        }

        // Could not find a branch named derived from either `master` or `main`.
        // This configuration is considered invalid.
        if computed_base.is_empty() {
            return Err(Args::command().error(
                clap::error::ErrorKind::ValueValidation,
                format!("Cannot find a 'master' or 'main' base branch with which to compare the current branch '{}'of the spec", current_branch),
            ));
        }
        computed_base
    };

    println!("Found '{}' as the base branch to compute diff", base_branch);
    // Finally, compute the diff between `current_branch` and `base_branch`.
    // Return the diff so we can inform the rewrapper of which lines to format
    // (as to avoid rewrapping the *entire* spec).
    let git_diff = std::process::Command::new("git")
        .arg("-C")
        .arg(directory)
        .arg("diff")
        .arg("-U0")
        .arg(format!("{base_branch}...{current_branch}"))
        .arg(filename_without_path)
        .output()
        .expect("Failed to compute `git diff`");

    Ok(String::from_utf8(git_diff.stdout).unwrap())
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
fn parse_diff_line_numbers(diff: &str, verbose: bool) -> Vec<usize> {
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
fn apply_diff(lines: &mut Vec<Line>, diff_line_numbers: &Vec<usize>, verbose: bool) {
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

fn main() {
    let args = Args::parse();
    let filename = default_filename(args.filename).unwrap_or_else(|err| err.exit());

    if !args.force {
        assert_no_uncommitted_changes(&filename).unwrap_or_else(|err| err.exit());
    }

    let diff = if !args.full_spec {
        git_diff(&filename, args.base_branch).unwrap_or_else(|err| err.exit())
    } else {
        String::from("")
    };
    let diff_line_numbers = if !args.full_spec {
        parse_diff_line_numbers(&diff, args.verbose)
    } else {
        Vec::new()
    };

    let (file, file_as_string): (File, String) = match read_file(&filename) {
        Ok((file, string)) => {
            println!("Successfully read file '{}'", filename.display());
            (file, string)
        }
        Err(error) => panic!("Error opening file '{}': {:?}", filename.display(), error),
    };

    let mut lines: Vec<Line> = file_as_string
        .split('\n')
        .map(|line_contents| Line {
            // If we are to format the entire spec, then mark each line as
            // subject to formatting.
            should_format: args.full_spec,
            contents: line_contents,
        })
        .collect();

    apply_diff(&mut lines, &diff_line_numbers, args.verbose);

    let num_lines_to_format = if args.full_spec {
        lines.len()
    } else {
        diff_line_numbers.len()
    };

    // Initiate unwrapping/rewrapping.
    let rewrapped_lines = rewrapper::rewrap_lines(lines, num_lines_to_format, args.wrap);

    // Join all lines and write to file.
    let file_as_string = rewrapped_lines.join("\n");
    match write_file(file, file_as_string) {
        Ok(_) => println!("Write succeeded"),
        Err(error) => panic!("Error writing file '{}': {:?}", filename.display(), error),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use test_generator::test_resources;

    #[test_resources("testcases/*.in.html")]
    fn simple_rewrap_tests(input: &str) {
        assert!(Path::new(input).exists());
        let output = input.replace("in.html", "out.html");
        assert!(Path::new(&output).exists());

        let (_in_file, in_string) = read_file(Path::new(input)).unwrap();
        let (_out_file, out_string) = read_file(Path::new(&output)).unwrap();

        let lines: Vec<Line> = in_string
            .split('\n')
            .map(|line| Line {
                should_format: true,
                contents: line,
            })
            .collect();
        let length = lines.len();

        // Initiate unwrapping/rewrapping.
        let wrapped_lines = rewrapper::rewrap_lines(lines, length, 100);
        let file_as_string: String = wrapped_lines.join("\n");

        let actual = input.replace("in.html", "actual.html");
        let actual_file  = OpenOptions::new()
            .write(true)
            .create(true)
            .open(Path::new(&actual))
            .unwrap();

        if file_as_string != out_string {
            // Only write the `-actual.html` file if there is a failure.
            match write_file(actual_file, file_as_string.clone()) {
                Ok(_) => println!("Write succeeded"),
                Err(error) => panic!("Error writing `-actual.html` file: {:?}", error),
            }
        } else {
            // And remove any existing `-actual.html` files for passing tests.
            if Path::new(&actual).exists() {
                std::fs::remove_file(Path::new(&actual)).unwrap();
            }
        }

        assert_eq!(file_as_string, out_string);
    }

    #[test_resources("testcases/git_diff/*.in.html")]
    fn git_diff_tests(input: &str) {
        assert!(Path::new(input).exists());
        let output = input.replace("in.html", "out.html");
        let diff = input.replace("in.html", "diff");
        assert!(Path::new(&output).exists());
        assert!(Path::new(&diff).exists());

        let (_in_file, in_string) = read_file(Path::new(input)).unwrap();
        let (_out_file, out_string) = read_file(Path::new(&output)).unwrap();
        let (_diff_file, diff_string) = read_file(Path::new(&diff)).unwrap();

        let mut lines: Vec<Line> = in_string
            .split('\n')
            .map(|line| Line {
                // Exempt all lines from formatting. `apply_diff()` below will
                // reverse this for lines included in the diff.
                should_format: false,
                contents: line,
            })
            .collect();
        let length = lines.len();

        let diff_line_numbers = parse_diff_line_numbers(&diff_string, false);
        apply_diff(&mut lines, &diff_line_numbers, false);

        // Initiate unwrapping/rewrapping.
        let wrapped_lines = rewrapper::rewrap_lines(lines, length, 100);
        let file_as_string: String = wrapped_lines.join("\n");

        let actual = input.replace("in.html", "actual.html");
        let actual_file  = OpenOptions::new()
            .write(true)
            .create(true)
            .open(Path::new(&actual))
            .unwrap();

        if file_as_string != out_string {
            // Only write the `-actual.html` file if there is a failure.
            match write_file(actual_file, file_as_string.clone()) {
                Ok(_) => println!("Write succeeded"),
                Err(error) => panic!("Error writing `-actual.html` file: {:?}", error),
            }
        } else {
            // And remove any existing `-actual.html` files for passing tests.
            if Path::new(&actual).exists() {
                std::fs::remove_file(Path::new(&actual)).unwrap();
            }
        }

        assert_eq!(file_as_string, out_string);
    }
}
