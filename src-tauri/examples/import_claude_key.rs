//! Dev helper: copy an Anthropic key from a .env file into the same Windows
//! Credential Manager entry Baa reads (service "rip.baa.app", user "anthropic").
//! The key is never printed.
//!
//!     cargo run --example import_claude_key -- C:/path/to/.env

fn main() {
    let path = std::env::args().nth(1).expect("pass the path to a .env file");
    let text = std::fs::read_to_string(&path).expect("can't read the .env file");
    let key = text
        .lines()
        .find_map(|l| l.strip_prefix("ANTHROPIC_API_KEY="))
        .map(|k| k.trim().trim_matches('"').to_string())
        .filter(|k| !k.is_empty())
        .expect("no ANTHROPIC_API_KEY line in that file");
    let entry = keyring::Entry::new("rip.baa.app", "anthropic").expect("keyring");
    entry.set_password(&key).expect("couldn't save to Credential Manager");
    let back = entry.get_password().expect("couldn't read it back");
    println!("saved: {} characters, read back {}", key.len(), if back == key { "OK" } else { "MISMATCH" });
}
