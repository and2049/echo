//! What the old `spotify` command turns into after an upgrade from v0.6.7 or earlier.

fn main() {
    eprintln!(
        "The echo terminal client has been removed. Use the echo desktop app (echo-desktop) instead."
    );
    std::process::exit(1);
}
