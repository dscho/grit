// API docs: https://docs.rs/grit-lib/latest/grit_lib/write_tree/fn.write_tree_from_index.html
use grit_lib::index::Index;
use grit_lib::odb::Odb;
use grit_lib::write_tree::write_tree_from_index;
use std::path::Path;

fn main() -> grit_lib::error::Result<()> {
    let odb = Odb::new(Path::new(".git/objects"));
    let index = Index::new();
    let tree_id = write_tree_from_index(&odb, &index, "")?;

    println!("empty tree: {tree_id}");
    Ok(())
}
