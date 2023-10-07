use std::path::PathBuf;

use clap::Parser as CliParser;

use onchg::parser::Parser;

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

    let parser = match &cli.mode {
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
        println!("Files checked:");
        for f in files {
            println!("  * {}", f.display());
        }
    } else {
        println!("No staged files to check");
        return;
    }

    println!();

    match &cli.mode {
        Mode::Repo { .. } => {
            let violations = parser.validate_git_repo();
            if let Err(e) = &violations {
                eprintln!("Failed to validate Git repo state: {}", e);
            }
            let violations = violations.unwrap();
            if violations.len() == 0 {
                println!("OK.");
                return;
            } else {
                println!("Violations:");
            }
            for v in violations {
                println!("  * {}", v.to_string());
            }
            std::process::exit(1);
        }
        _ => (),
    }
}
