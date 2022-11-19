use std::io::BufRead;

#[allow(unused_imports)]
use {
    anyhow::{Context, Result},
    clap::Parser,
    dotenv::dotenv,
    // requestty::{prompt_one, Answer, Question},
    std::{fs::File, io::BufReader, path::PathBuf},
    tracing::{debug, info},
};

#[derive(Parser, Debug)]
struct Cli {
    /// The log file to process
    #[arg(name = "Log File", long, short, env = "LOG_FILE")]
    path: Option<PathBuf>,
}

fn main() -> Result<()> {
    dotenv().ok();

    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }

    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    let path = &cli.path.unwrap_or("data/test.log".into());
    info!("Using {}", path.display());
    let file = File::open(path).with_context(|| format!("could not open file `{:?}`", path))?;
    let buffer = BufReader::new(file);

    buffer
        .lines()
        .filter_map(|s| s.ok())
        .map(|s| String::from(&s[33..]))
        .filter(|s| !s.starts_with("--"))
        .for_each(|s| println!("{}", s));

    Ok(())
}
