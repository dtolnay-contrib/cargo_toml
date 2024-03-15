//! If you want a complex example, see
//! <https://gitlab.com/lib.rs/main/-/blob/6181e6a1db23f03586ff0d6006c42727c817b933/crate_git_checkout/src/crate_git_checkout.rs#L225-255>

use cargo_toml::Manifest;
use std::path::PathBuf;

fn main() {
    let path = std::env::args_os().nth(1).map(PathBuf::from)
        .expect("Please specify path to Cargo.toml");

    let manifest = Manifest::from_path(path).unwrap();

    eprintln!("{manifest:#?}");
}
