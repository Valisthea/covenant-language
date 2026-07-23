use clap::Parser;
use covenant_lint::cli::Cli;

fn main() {
    let args = Cli::parse();

    if args.paths.is_empty() {
        eprintln!("covenant-lint: no input files specified");
        std::process::exit(2);
    }

    match covenant_lint::run(&args) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("covenant-lint: {e:#}");
            std::process::exit(2);
        }
    }
}
