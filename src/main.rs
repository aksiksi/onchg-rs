use std::path::{Path, PathBuf};

use clap::Parser as CliParser;

use onchg::Parser;

const DEFAULT_MAX_FILES_TO_DISPLAY: usize = 15;
const DEFAULT_MAX_VIOLATIONS_TO_DISPLAY: usize = 10;

fn default_path() -> PathBuf {
    PathBuf::from(".")
}

#[derive(clap::Parser, Clone, Debug)]
enum Mode {
    /// Validate changes to staged files in a Git repo.
    ///
    /// This looks at any staged file(s) and ensures that all block
    /// targets in the staged file(s) are also staged. Unlike in "directory"
    /// mode, ignore files are _not_ checked.
    Repo {
        #[arg(required = false, default_value = default_path().into_os_string())]
        path: PathBuf,
    },
    /// Check all files in a directory. By default, this will skip parsing any files
    /// specified in the various ignore files.
    ///
    /// See the [ignore] crate for more details.
    Directory {
        #[arg(required = false, default_value = default_path().into_os_string())]
        path: PathBuf,

        /// Do not adhere to Git ignore files.
        #[arg(long, default_value_t = false)]
        no_ignore: bool,
    },
}

#[derive(clap::Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[clap(subcommand)]
    mode: Mode,

    #[arg(short, long)]
    verbose: bool,

    #[arg(long, default_value_t = DEFAULT_MAX_FILES_TO_DISPLAY, global = true)]
    max_files_to_display: usize,

    /// Do not log anything to stdout.
    #[arg(short, long, global = true)]
    quiet: bool,
}

fn main() {
    env_logger::init();

    let cli = Cli::parse();

    let parser = match &cli.mode {
        Mode::Directory { path, no_ignore } => Parser::from_directory(path, !no_ignore),
        Mode::Repo { path, .. } => Parser::from_git_repo(path),
    };
    if let Err(e) = parser {
        eprintln!("Parsing failed: {}", e);
        std::process::exit(1);
    }
    let parser = parser.unwrap();

    let mut files: Vec<&Path> = parser.paths().collect();
    files.sort();

    if !cli.quiet {
        println!("Root path: {}\n", parser.root_path().display());
    }

    if !cli.quiet {
        if files.len() != 0 {
            println!(
                "Parsed {} files ({} blocks total):",
                files.len(),
                parser.num_blocks(),
            );
            for f in files.iter().take(DEFAULT_MAX_FILES_TO_DISPLAY) {
                println!("  * {}", parser.root_path().join(f).display());
            }
            if files.len() > DEFAULT_MAX_FILES_TO_DISPLAY {
                println!(
                    "  ... {} files omitted",
                    files.len() - DEFAULT_MAX_FILES_TO_DISPLAY,
                );
            }
        } else if let Mode::Repo { .. } = cli.mode {
            println!("No staged files to check.");
            return;
        }
    }

    println!();

    match &cli.mode {
        Mode::Repo { .. } => {
            let violations = parser.validate_git_repo();
            if let Err(e) = &violations {
                eprintln!("Failed to validate Git repo state: {}", e);
                std::process::exit(1);
            }
            let violations = violations.unwrap();
            if violations.len() != 0 {
                eprintln!("Violations:");
                for v in violations.iter().take(DEFAULT_MAX_VIOLATIONS_TO_DISPLAY) {
                    eprintln!("  * {}", v.to_string());
                }
                if violations.len() > DEFAULT_MAX_FILES_TO_DISPLAY {
                    println!(
                        "  ... {} violations omitted",
                        violations.len() - DEFAULT_MAX_VIOLATIONS_TO_DISPLAY,
                    );
                }
                std::process::exit(1);
            }
        }
        _ => (),
    };

    if !cli.quiet {
        println!("OK.");
    }
}
