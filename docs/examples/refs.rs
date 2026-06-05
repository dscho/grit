// API docs: https://docs.rs/grit-lib/latest/grit_lib/refs/index.html
use grit_lib::refs::{list_refs, read_head};
use std::path::Path;

fn main() -> grit_lib::error::Result<()> {
    let git_dir = Path::new(".git");
    println!("HEAD = {:?}", read_head(git_dir)?);

    for (name, oid) in list_refs(git_dir, "refs/heads")? {
        println!("{name} {oid}");
    }
    Ok(())
}
