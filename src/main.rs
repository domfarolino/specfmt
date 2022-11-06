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

    /// Reformatting the spec even if it has uncommitted changes.
    #[arg(short, long, default_value_t = false)]
    ignore_uncommitted_changes: bool,
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

fn assert_no_uncommitted_changes(path: &PathBuf) -> Result<(), clap::error::Error> {
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

fn main() {
    let args = Args::parse();
    let filename = default_filename(args.filename).unwrap_or_else(|err| err.exit());

    if !args.ignore_uncommitted_changes {
      assert_no_uncommitted_changes(&filename).unwrap_or_else(|err| err.exit());
    }

    let (file, file_as_string): (File, String) = match read_file(&filename) {
        Ok((file, string)) => {
            println!("Successfully read file '{}'", filename.display());
            (file, string)
        }
        Err(error) => panic!("Error opening file '{}': {:?}", filename.display(), error),
    };

    let lines: Vec<&str> = file_as_string.split("\n").collect();

    // Initiate unwrapping/rewrapping.
    let rewrapped_lines = rewrapper::rewrap_lines(lines, args.wrap);

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
    fn verify_resource(input: &str) {
        assert!(Path::new(input).exists());
        let output = input.replace("in.html", "out.html");
        assert!(Path::new(&output).exists());

        let (_in_file, in_string) = read_file(Path::new(input)).unwrap();
        let (_out_file, out_string) = read_file(Path::new(&output)).unwrap();

        let lines: Vec<&str> = in_string.split("\n").collect();

        // Initiate unwrapping/rewrapping.
        let wrapped_lines = rewrapper::rewrap_lines(lines, 100);
        let file_as_string: String = wrapped_lines.join("\n");
        assert_eq!(file_as_string, out_string);
    }
}
