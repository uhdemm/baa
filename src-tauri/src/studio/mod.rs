//! Character Studio: make your own buddy by describing it. The pipeline comes from
//! Sprite Studio (github.com/JohnKinyanjui/sprite-maker, MIT, by John Kinyanjui),
//! cut down to what a Baa character needs:
//! 1. one master pose, from your description (and any pictures you attach);
//! 2. you approve it (or try again);
//! 3. one key-pose sheet per mood, each drawn with the master as the identity reference;
//! 4. every sheet is cleaned, cut, stood on one ground line at one scale, checked,
//!    and packed; a sheet that fails the check is redrawn once;
//! 5. the pack is installed and ready to pick.
//! Projects live in <app data>/studio/<id>/ so a half-done character can be resumed.

pub mod codex;
pub mod cut;
pub mod prompts;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub description: String,
    /// pictures the user attached (copied into the project)
    pub refs: Vec<String>,
    pub master_ok: bool,
    /// pixel-art character: drawn on a real pixel grid, shown crisp
    #[serde(default)]
    pub pixel: bool,
}

#[derive(Serialize, Clone)]
pub struct Progress {
    pub id: String,
    /// "master" | "sheets" | "pack" | "done" | "error"
    pub step: String,
    pub done: usize,
    pub total: usize,
    pub message: String,
}

pub fn project_dir(root: &Path, id: &str) -> PathBuf {
    root.join(id)
}

pub fn slug(name: &str) -> String {
    let s: String = name.trim().to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let s = s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
    if s.is_empty() { "my-character".into() } else { s.chars().take(40).collect() }
}

pub fn load(root: &Path, id: &str) -> anyhow::Result<Project> {
    Ok(serde_json::from_slice(&std::fs::read(project_dir(root, id).join("project.json"))?)?)
}

pub fn save(root: &Path, p: &Project) -> anyhow::Result<()> {
    let d = project_dir(root, &p.id);
    std::fs::create_dir_all(&d)?;
    std::fs::write(d.join("project.json"), serde_json::to_vec_pretty(p)?)?;
    Ok(())
}

/// Starts (or restarts) a character: saves the brief and copies the attached pictures in.
pub fn create(root: &Path, name: &str, description: &str, refs: &[PathBuf], pixel: bool) -> anyhow::Result<Project> {
    let id = slug(name);
    let d = project_dir(root, &id);
    std::fs::create_dir_all(d.join("refs"))?;
    let mut kept = vec![];
    for (i, r) in refs.iter().enumerate() {
        let ext = r.extension().and_then(|e| e.to_str()).unwrap_or("png").to_lowercase();
        if !["png", "jpg", "jpeg", "webp"].contains(&ext.as_str()) {
            continue;
        }
        let to = d.join("refs").join(format!("ref-{}.{ext}", i + 1));
        std::fs::copy(r, &to)?;
        kept.push(to.to_string_lossy().into_owned());
    }
    let p = Project { id, name: name.trim().to_string(), description: description.trim().to_string(), refs: kept, master_ok: false, pixel };
    save(root, &p)?;
    Ok(p)
}

/// Step 1: draw the master pose (again).
pub fn draw_master(root: &Path, p: &Project, progress: &impl Fn(Progress)) -> anyhow::Result<PathBuf> {
    let d = project_dir(root, &p.id);
    let out = d.join("master.png");
    let tell = |m: &str| progress(Progress { id: p.id.clone(), step: "master".into(), done: 0, total: 1, message: m.into() });
    tell("Drawing your character…");
    let refs: Vec<PathBuf> = p.refs.iter().map(PathBuf::from).collect();
    codex::generate(&prompts::master_prompt(&p.description, !refs.is_empty(), p.pixel, &fwd(&out)), &out, &refs, &d, |m| tell(m))?;
    // a painted checkerboard or flat background becomes real transparency now, so the
    // moves (drawn from this picture) don't copy the background too
    let clean = cut::clean(&image::open(&out)?.to_rgba8());
    clean.save(&out)?;
    Ok(out)
}

/// A picture as a data: URL the page can show directly, shrunk to fit `max` px.
pub fn data_url(path: &Path, max: u32) -> anyhow::Result<String> {
    use base64::Engine;
    let mut img = image::open(path)?;
    if img.width() > max || img.height() > max {
        img = img.resize(max, max, image::imageops::FilterType::Triangle);
    }
    let mut png = std::io::Cursor::new(Vec::new());
    img.write_to(&mut png, image::ImageFormat::Png)?;
    Ok(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png.into_inner())))
}

fn fwd(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// Steps 3-5: every mood sheet (two at a time), checked, packed, installed into `characters`.
/// Sheets already drawn are reused, so a stopped run picks up where it left off.
pub fn draw_moves(root: &Path, characters: &Path, p: &Project, progress: &(impl Fn(Progress) + Sync)) -> anyhow::Result<String> {
    let d = project_dir(root, &p.id);
    let master = d.join("master.png");
    if !master.exists() {
        anyhow::bail!("Draw and approve the character first");
    }
    std::fs::create_dir_all(d.join("sheets"))?;
    let total = prompts::MOODS.len();
    let done = std::sync::atomic::AtomicUsize::new(prompts::MOODS.iter().filter(|m| d.join("sheets").join(format!("{}.png", m.id)).exists()).count());
    let tell = |msg: String| {
        progress(Progress { id: p.id.clone(), step: "sheets".into(), done: done.load(std::sync::atomic::Ordering::SeqCst), total, message: msg })
    };
    tell("Drawing the moves…".into());

    // a sheet that comes back unusable (merged or missing frames) gets one more try
    let draw = |m: &prompts::Mood| -> anyhow::Result<()> {
        let out = d.join("sheets").join(format!("{}.png", m.id));
        if out.exists() && cut::cut_sheet(&out, 6, p.pixel).is_ok() {
            return Ok(());
        }
        let mut last = None;
        for attempt in 0..2 {
            tell(format!("Drawing \"{}\"{}…", m.id, if attempt > 0 { " again" } else { "" }));
            match codex::generate(&prompts::sheet_prompt(m, p.pixel, &fwd(&out)), &out, &[master.clone()], &d, |_| {}).and_then(|_| cut::cut_sheet(&out, 6, p.pixel).map(|_| ())) {
                Ok(()) => {
                    done.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    tell(format!("\"{}\" done", m.id));
                    return Ok(());
                }
                Err(e) => {
                    // the safety filter says the same thing every time, so don't spend another go on it
                    let refused = e.to_string().contains(codex::REFUSED);
                    last = Some(e);
                    if refused {
                        break;
                    }
                }
            }
        }
        Err(last.unwrap())
    };
    // two lanes, so the whole set takes about half as long
    let lanes: [Vec<&prompts::Mood>; 2] = [
        prompts::MOODS.iter().step_by(2).collect(),
        prompts::MOODS.iter().skip(1).step_by(2).collect(),
    ];
    let errs = std::sync::Mutex::new(Vec::<(&str, String)>::new());
    std::thread::scope(|s| {
        for lane in lanes {
            let (draw, errs) = (&draw, &errs);
            s.spawn(move || {
                for m in lane {
                    if let Err(e) = draw(m) {
                        errs.lock().unwrap().push((m.id, e.to_string()));
                    }
                }
            });
        }
    });
    // the extras (fidget, dance) may fail without stopping the character from being made
    let errs: Vec<(&str, String)> = errs.into_inner().unwrap().into_iter().filter(|(id, _)| !prompts::OPTIONAL.contains(id)).collect();
    if !errs.is_empty() {
        let (refused, other): (Vec<_>, Vec<_>) = errs.iter().partition(|(_, e)| e.contains(codex::REFUSED));
        if other.is_empty() {
            anyhow::bail!(
                "OpenAI's image safety filter wouldn't draw these moves: {}. Pressing again won't change its mind; \
a different description or picture (more clothes, a less revealing look) usually gets through.",
                refused.iter().map(|(id, _)| *id).collect::<Vec<_>>().join(", ")
            );
        }
        let list = errs.iter().map(|(id, e)| format!("{id}: {e}")).collect::<Vec<_>>().join("; ");
        anyhow::bail!("Some moves didn't come out right ({list}). Press Make the moves again to retry just those.");
    }

    progress(Progress { id: p.id.clone(), step: "pack".into(), done: total, total, message: "Putting it together…".into() });
    let mut sheets = vec![];
    for m in prompts::MOODS {
        let f = d.join("sheets").join(format!("{}.png", m.id));
        if !f.exists() && prompts::OPTIONAL.contains(&m.id) {
            continue;
        }
        sheets.push((m.id.to_string(), cut::cut_sheet(&f, 6, p.pixel)?));
    }
    let meta = serde_json::json!({"id": p.id, "name": p.name, "author": "", "description": p.description, "pixel": p.pixel});
    let out = characters.join(&p.id);
    if out.exists() {
        std::fs::remove_dir_all(&out)?;
    }
    cut::write_pack(&out, &meta, &sheets, cut::standard_states())?;
    progress(Progress { id: p.id.clone(), step: "done".into(), done: total, total, message: format!("{} is ready", p.name) });
    Ok(p.id.clone())
}

/// Every project, newest first, for the studio's list.
pub fn list(root: &Path) -> Vec<Project> {
    let mut v: Vec<(std::time::SystemTime, Project)> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p: Project = serde_json::from_slice(&std::fs::read(e.path().join("project.json")).ok()?).ok()?;
            Some((e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH), p))
        })
        .collect();
    v.sort_by(|a, b| b.0.cmp(&a.0));
    v.into_iter().map(|(_, p)| p).collect()
}
