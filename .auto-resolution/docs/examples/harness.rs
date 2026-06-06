// API docs: https://docs.rs/grit-lib/latest/grit_lib/repo/struct.Repository.html
use std::process::Command;

fn main() -> std::io::Result<()> {
    let output = Command::new("./scripts/run-tests.sh")
        .arg("t0000-basic.sh")
        .output()?;

    println!("{}", String::from_utf8_lossy(&output.stdout));
    Ok(())
}
