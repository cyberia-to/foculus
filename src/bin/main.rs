// ---
// tags: foculus, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! The `foculus` binary — a one-liner over [`foculus::cli`].

fn main() -> anyhow::Result<()> {
    foculus::cli::run()
}
