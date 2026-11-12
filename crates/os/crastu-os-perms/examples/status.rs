//! Prints the status of each permission for this process (never prompts):
//! `cargo run -p crastu-os-perms --example status`.

use crastu_os_perms::{Permission, status};

fn main() {
    for p in Permission::ALL {
        println!("{p:?}: {:?}", status(p));
    }
}
