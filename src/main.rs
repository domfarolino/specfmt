use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;

// Adapted from the web version of the original rewrapper
// (https://github.com/domenic/rewrapper).

mod rewrapper;

fn read_file(filename: &str) -> Result<(File, String), io::Error> {
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

fn main() {
    let filename: &str = "source";
    let column_length: u8 = 100;
    let (file, file_as_string): (File, String) = match read_file(filename) {
        Ok((file, string)) => {
            println!("Successfully read file '{}'", filename);
            (file, string)
        }
        Err(error) => panic!("Error opening file '{}': {:?}", filename, error),
    };

    let lines: Vec<&str> = file_as_string.split("\n").collect();

    // Initiate unwrapping/rewrapping.
    let wrapped_lines = rewrapper::wrap_lines(&lines, column_length);

    // Join all lines and write to file.
    let file_as_string = wrapped_lines.join("\n");
    match write_file(file, file_as_string) {
        Ok(_) => println!("Write succeeded"),
        Err(error) => panic!("Error writing file '{}': {:?}", filename, error),
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use test_generator::test_resources;
    #[test_resources("testcases/*.in.html")]
    fn verify_resource(input: &str) {
        assert!(std::path::Path::new(input).exists());
        let output = input.replace("in.html", "out.html");
        assert!(std::path::Path::new(&output).exists());

        let (_in_file, in_string) = read_file(input).unwrap();
        let (_out_file, out_string) = read_file(&output).unwrap();

        let lines: Vec<&str> = in_string.split("\n").collect();

        // Initiate unwrapping/rewrapping.
        let wrapped_lines = rewrapper::wrap_lines(&lines, 100);
        let file_as_string: String = wrapped_lines.join("\n");
        assert_eq!(file_as_string, out_string);
    }
}
