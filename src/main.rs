use std::path::PathBuf;

mod core;
mod git;
mod parser;
#[cfg(test)]
mod test_helpers;

use clap::Parser as CliParser;
use parser::Parser;

#[derive(clap::Parser, Clone, Debug)]
enum Mode {
    Repo { path: PathBuf },
    Directory { path: PathBuf },
}

#[derive(clap::Parser, Debug)]
struct Cli {
    #[clap(subcommand)]
    mode: Mode,

    #[clap(short, long)]
    verbose: bool,
}

fn main() {
    let cli = Cli::parse();

    let parser = match cli.mode {
        Mode::Directory { path } => Parser::from_directory(path),
        Mode::Repo { path } => Parser::from_git_repo(path),
    };
    if let Err(e) = parser {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    let parser = parser.unwrap();
    let mut files = parser.files();
    files.sort();

    println!("Root path: {}\n", parser.root_path().display());

    if files.len() != 0 {
        println!("Checked:");
        for f in files {
            println!("  * {}", f.display());
        }
    } else {
        println!("No files to check");
    }
}
