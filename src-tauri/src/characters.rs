//! Character packs: the sprite that talks when you talk. The Sheep ships inside
//! the app; packs made in Character Studio are installed straight here, and shared
//! ones are .baa
//! files (a zip of character.json + sprite sheets) installed into
//! <app data>/characters/<id>/, where the webview reads them over the asset protocol.

use serde::Serialize;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const BUILTIN: &[&str] = &["sheep"];

#[derive(Serialize, Clone)]
pub struct CharacterInfo {
    pub id: String,
    pub name: String,
    pub author: String,
    pub description: String,
    /// folder on disk for installed packs; None for built-in ones (served from the app)
    pub dir: Option<String>,
}

/// Only plain names: a pack can't write outside its own folder.
fn safe_id(id: &str) -> Option<String> {
    let id: String = id.trim().to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' }).collect();
    let id = id.trim_matches('-').to_string();
    (!id.is_empty() && id.len() <= 48 && !BUILTIN.contains(&id.as_str())).then_some(id)
}

fn allowed_file(name: &str) -> bool {
    let n = name.to_lowercase();
    !n.contains('/') && !n.contains('\\') && !n.contains("..") && (n == "character.json" || n.ends_with(".webp") || n.ends_with(".png"))
}

fn info_from(json: &serde_json::Value, dir: Option<String>) -> Option<CharacterInfo> {
    let s = |k: &str| json.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    if json.get("format").and_then(|v| v.as_i64()) != Some(1) || json.get("states").is_none() {
        return None;
    }
    Some(CharacterInfo { id: s("id"), name: s("name"), author: s("author"), description: s("description"), dir })
}

pub fn list(dir: &Path) -> Vec<CharacterInfo> {
    let mut out = vec![];
    let Ok(rd) = std::fs::read_dir(dir) else { return out };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(text) = std::fs::read_to_string(p.join("character.json")) else { continue };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
        if let Some(mut info) = info_from(&json, Some(p.to_string_lossy().into_owned())) {
            info.id = e.file_name().to_string_lossy().into_owned();
            out.push(info);
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// Installs a .baa file (or a folder with character.json in it). Returns the pack's id.
pub fn import(dir: &Path, src: &Path) -> anyhow::Result<String> {
    let mut files: Vec<(String, Vec<u8>)> = vec![];
    if src.is_dir() {
        for e in std::fs::read_dir(src)?.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if e.path().is_file() && allowed_file(&name) {
                files.push((name, std::fs::read(e.path())?));
            }
        }
    } else {
        let mut zip = zip::ZipArchive::new(std::fs::File::open(src)?).map_err(|_| anyhow::anyhow!("That isn't a Baa character file"))?;
        for i in 0..zip.len() {
            let mut f = zip.by_index(i)?;
            let name = f.name().rsplit('/').next().unwrap_or("").to_string();
            if f.is_file() && allowed_file(&name) {
                if f.size() > 40 * 1024 * 1024 {
                    anyhow::bail!("{name} is too big for a character sheet");
                }
                let mut buf = vec![];
                f.read_to_end(&mut buf)?;
                files.push((name, buf));
            }
        }
    }
    let json_bytes = files.iter().find(|(n, _)| n.eq_ignore_ascii_case("character.json")).map(|(_, b)| b.clone()).ok_or_else(|| anyhow::anyhow!("No character.json in it"))?;
    let json: serde_json::Value = serde_json::from_slice(&json_bytes).map_err(|e| anyhow::anyhow!("character.json is broken: {e}"))?;
    let info = info_from(&json, None).ok_or_else(|| anyhow::anyhow!("This character was made for a newer Baa"))?;
    // every sheet the pack names has to be in the file
    if let Some(sheets) = json.get("sheets").and_then(|v| v.as_object()) {
        for s in sheets.values() {
            let f = s.get("file").and_then(|v| v.as_str()).unwrap_or("");
            if !files.iter().any(|(n, _)| n == f) {
                anyhow::bail!("The pack is missing {f}");
            }
        }
    }
    let id = safe_id(&info.id).or_else(|| safe_id(&info.name)).ok_or_else(|| anyhow::anyhow!("The character needs a name"))?;
    let target: PathBuf = dir.join(&id);
    if target.exists() {
        std::fs::remove_dir_all(&target)?; // re-importing replaces the old version
    }
    std::fs::create_dir_all(&target)?;
    for (name, bytes) in files {
        std::fs::write(target.join(name), bytes)?;
    }
    Ok(id)
}

pub fn delete(dir: &Path, id: &str) -> anyhow::Result<()> {
    let id = safe_id(id).ok_or_else(|| anyhow::anyhow!("Built-in characters can't be removed"))?;
    let p = dir.join(id);
    if p.exists() {
        std::fs::remove_dir_all(p)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_files_stay_inside_the_pack() {
        assert_eq!(safe_id("Frog Prince!").as_deref(), Some("frog-prince"));
        assert_eq!(safe_id("../../evil").as_deref(), Some("evil"));
        assert!(safe_id("sheep").is_none());
        assert!(allowed_file("talk.webp") && allowed_file("character.json"));
        assert!(!allowed_file("../x.webp") && !allowed_file("run.exe") && !allowed_file("a/b.png"));
    }
}
