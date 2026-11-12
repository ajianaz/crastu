//! `crastu-cli`: the commands of [`crastu_cli`] without a window.

fn main() -> std::process::ExitCode {
    crastu_cli::run("crastu-cli", std::env::args().skip(1).collect())
}
