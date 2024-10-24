use clap::Parser;
use xshell::Shell;

type Error = anyhow::Error;
type Result<T = (), E = Error> = core::result::Result<T, E>;

mod args;
mod build;
mod changelog;
mod checks;
mod publish;
mod tests;

fn main() {
    let sh = Shell::new().unwrap();
    if let Err(err) = args::Args::parse().run(&sh) {
        eprintln!("{err:?}");
        std::process::exit(1);
    }
}
