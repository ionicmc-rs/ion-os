#![deprecated(since = "0.0.0", reason = "never used")]
use std::process::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Command::new("run.sh")
        .status();
    Ok(())
}