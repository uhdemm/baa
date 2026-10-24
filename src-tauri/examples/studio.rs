//! Run Character Studio without the app (same code the Studio page uses).
//!
//!     cargo run --example studio -- master <projects dir> <name> "<description>" [ref.png ...]
//!     cargo run --example studio -- moves  <projects dir> <characters dir> <id>
//!     cargo run --example studio -- pack   <projects dir> <characters dir> <id>    (re-pack drawn sheets only)

use baa_lib::studio;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let tell = |p: studio::Progress| println!("[{} {}/{}] {}", p.step, p.done, p.total, p.message);
    match a.first().map(String::as_str) {
        Some("master") => {
            let root = PathBuf::from(&a[1]);
            let refs: Vec<PathBuf> = a[4..].iter().map(PathBuf::from).collect();
            let p = studio::create(&root, &a[2], &a[3], &refs)?;
            let out = studio::draw_master(&root, &p, &tell)?;
            println!("master: {}  (id {})", out.display(), p.id);
        }
        Some("moves") => {
            let (root, chars) = (PathBuf::from(&a[1]), PathBuf::from(&a[2]));
            let p = studio::load(&root, &a[3])?;
            println!("installed: {}", studio::draw_moves(&root, &chars, &p, &tell)?);
        }
        Some("pack") => {
            let (root, chars) = (PathBuf::from(&a[1]), PathBuf::from(&a[2]));
            let p = studio::load(&root, &a[3])?;
            let d = studio::project_dir(&root, &p.id).join("sheets");
            let mut sheets = vec![];
            for m in studio::prompts::MOODS {
                let f = d.join(format!("{}.png", m.id));
                if f.exists() {
                    sheets.push((m.id.to_string(), studio::cut::cut_sheet(&f, 6)?));
                    println!("cut {}", m.id);
                }
            }
            let meta = serde_json::json!({"id": p.id, "name": p.name, "author": "", "description": p.description});
            studio::cut::write_pack(&chars.join(&p.id), &meta, &sheets, studio::cut::standard_states())?;
            println!("packed into {}", chars.join(&p.id).display());
        }
        _ => println!("see the top of examples/studio.rs for usage"),
    }
    Ok(())
}
