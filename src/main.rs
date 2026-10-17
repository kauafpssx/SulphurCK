//! Binary composition root.
//!
//! Framework setup and delivery mechanisms live at the edge of the application.
//! Keeping this file tiny makes the reusable library independent of the CLI.

fn main() {
    sulphur_ck::presentation::cli::run();
}
