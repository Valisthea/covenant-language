//! `covenant explain <code>` — print the long-form prose for a diagnostic code.
//!
//! V0.9 Sprint 38 Phase 38.1.b. Backed by `covenant_diag::explanations`.

use clap::Args;

use crate::error::CliError;

#[derive(Args, Debug, Clone)]
pub struct ExplainArgs {
    /// Diagnostic code to explain. Accepts either the numeric form
    /// (`421`) or the prefixed form (`E421`, `e421`, `W606`, `w606`).
    #[arg(value_name = "CODE")]
    pub code: Option<String>,

    /// List all known diagnostic codes with their summaries.
    #[arg(long, conflicts_with = "code")]
    pub list: bool,
}

pub fn run(args: ExplainArgs) -> Result<(), CliError> {
    if args.list {
        print_list();
        return Ok(());
    }

    let raw = match args.code {
        Some(c) => c,
        None => {
            eprintln!(
                "error: missing argument <CODE>\n\n\
                 Usage:\n\
                   covenant explain <CODE>      # e.g. covenant explain E421\n\
                   covenant explain --list      # see all known codes"
            );
            return Err(CliError::Usage("invalid diagnostic code".to_string()));
        }
    };

    let n = parse_code(&raw)?;

    match covenant_diag::explanations::lookup_by_number(n) {
        Some(exp) => {
            println!("{}", exp.to_terminal_string());
            println!();
            println!("(more codes available — try `covenant explain --list`)");
            Ok(())
        }
        None => {
            eprintln!(
                "error: no long-form explanation registered for code {n}\n\
                 \n\
                 The diagnostic was emitted but its prose is not yet curated. If\n\
                 you can describe what the message means in your case, please\n\
                 contribute via github.com/Valisthea/covenant-language/issues — every\n\
                 frequent-error code that lands here makes the language easier\n\
                 to learn.\n\
                 \n\
                 In the meantime, see `covenant explain --list` for the codes\n\
                 that ARE documented."
            );
            Err(CliError::CompileError)
        }
    }
}

fn print_list() {
    let all = covenant_diag::explanations::all();
    if all.is_empty() {
        println!("(no explanations registered)");
        return;
    }
    println!("Known diagnostic codes ({})\n", all.len());
    println!("  {:<8} {:<20} summary", "code", "category");
    println!("  {:<8} {:<20} -------", "----", "--------");
    for exp in all {
        println!(
            "  E{:<7} {:<20} {}",
            exp.code,
            exp.category,
            truncate(exp.summary, 60)
        );
    }
    println!();
    println!("Run `covenant explain <code>` for the full prose.");
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}…", &s[..n - 1])
    }
}

fn parse_code(raw: &str) -> Result<u32, CliError> {
    let trimmed = raw.trim();
    let stripped = trimmed
        .strip_prefix(['E', 'e', 'W', 'w'])
        .unwrap_or(trimmed);
    stripped.parse::<u32>().map_err(|_| {
        eprintln!(
            "error: could not parse '{raw}' as a diagnostic code. Use a \
             numeric form like `421` or `E421` / `W606`."
        );
        CliError::Usage("invalid diagnostic code".to_string())
    })
}
