//! The `lintgraph` command. The only crate that wires the others together.

fn main() {
    println!("lintgraph {}", env!("CARGO_PKG_VERSION"));
}
