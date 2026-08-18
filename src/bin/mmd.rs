//! The `mmd` command-line interface.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;

/// Render a Mermaid diagram to SVG.
#[derive(Debug, Parser)]
#[command(name = "mmd", version, about)]
struct Args {
    /// Diagram file; `-` reads standard input.
    input: PathBuf,

    /// Where to write the SVG; `-` writes to standard output.
    ///
    /// Defaults to the input path with an `.svg` extension.
    #[arg(short, long)]
    output: Option<PathBuf>,
}

fn main() -> ExitCode {
    let args = Args::parse();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("mmd: {}: {message}", args.input.display());
            ExitCode::FAILURE
        }
    }
}

fn run(args: &Args) -> Result<(), String> {
    let source = read_input(&args.input)?;
    let svg = mmd::render(&source).map_err(|error| error.to_string())?;

    match args.output.as_deref() {
        Some(path) if path == Path::new("-") => {
            let mut stdout = std::io::stdout().lock();
            stdout
                .write_all(svg.as_bytes())
                .map_err(|error| error.to_string())?;
        }
        Some(path) => std::fs::write(path, &svg).map_err(|error| error.to_string())?,
        None if args.input == Path::new("-") => {
            return Err("reading standard input needs an explicit --output".to_string());
        }
        None => {
            let path = args.input.with_extension("svg");
            std::fs::write(&path, &svg).map_err(|error| error.to_string())?;
            eprintln!("{}", path.display());
        }
    }
    Ok(())
}

fn read_input(path: &Path) -> Result<String, String> {
    if path == Path::new("-") {
        let mut source = String::new();
        std::io::stdin()
            .read_to_string(&mut source)
            .map_err(|error| error.to_string())?;
        return Ok(source);
    }
    std::fs::read_to_string(path).map_err(|error| error.to_string())
}
