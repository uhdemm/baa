//! Speech model catalog, downloads (with progress + checksum), imports and deletes.

use futures_util::StreamExt;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct CatalogModel {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tier: String,
    pub repo: String,
    pub revision: String,
    pub filename: String,
    pub size: u64,
    pub sha256: Option<String>,
    pub license: Option<String>,
}

#[derive(Deserialize)]
struct Catalog {
    models: Vec<CatalogModel>,
}

pub fn catalog() -> Vec<CatalogModel> {
    let c: Catalog = serde_json::from_str(include_str!("catalog.json")).expect("catalog.json");
    c.models
}

#[derive(Serialize, Clone)]
pub struct ModelRow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tier: String,
    pub size: u64,
    pub installed: bool,
    pub downloading: bool,
    pub imported: bool,
    pub path: Option<String>,
}

#[derive(Serialize, Clone)]
struct Progress<'a> {
    id: &'a str,
    done: u64,
    total: u64,
    state: &'a str, // "downloading" | "verifying" | "done" | "error" | "cancelled"
    error: Option<String>,
}

pub struct Models {
    pub dir: PathBuf,
    active: Mutex<HashSet<String>>,
    cancel: Mutex<HashSet<String>>,
}

impl Models {
    pub fn new(dir: PathBuf) -> Arc<Self> {
        let _ = std::fs::create_dir_all(&dir);
        Arc::new(Models { dir, active: Mutex::new(HashSet::new()), cancel: Mutex::new(HashSet::new()) })
    }

    /// Resolves a settings model id to a file on disk.
    pub fn path_for(&self, id: &str) -> Option<PathBuf> {
        if let Some(p) = id.strip_prefix("file:") {
            let p = PathBuf::from(p);
            return p.exists().then_some(p);
        }
        let m = catalog().into_iter().find(|m| m.id == id)?;
        let p = self.dir.join(&m.filename);
        p.exists().then_some(p)
    }

    pub fn list(&self, imported: &[String]) -> Vec<ModelRow> {
        let active = self.active.lock().clone();
        let mut rows: Vec<ModelRow> = catalog()
            .into_iter()
            .map(|m| {
                let p = self.dir.join(&m.filename);
                ModelRow {
                    installed: p.exists(),
                    downloading: active.contains(&m.id),
                    path: p.exists().then(|| p.to_string_lossy().into_owned()),
                    id: m.id,
                    name: m.name,
                    description: m.description,
                    tier: m.tier,
                    size: m.size,
                    imported: false,
                }
            })
            .collect();
        for path in imported {
            let p = Path::new(path);
            rows.push(ModelRow {
                id: format!("file:{path}"),
                name: p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| path.clone()),
                description: path.clone(),
                tier: "imported".into(),
                size: std::fs::metadata(p).map(|m| m.len()).unwrap_or(0),
                installed: p.exists(),
                downloading: false,
                imported: true,
                path: Some(path.clone()),
            });
        }
        rows
    }

    pub fn cancel(&self, id: &str) {
        self.cancel.lock().insert(id.to_string());
    }

    pub fn delete(&self, id: &str) -> anyhow::Result<()> {
        let m = catalog().into_iter().find(|m| m.id == id).ok_or_else(|| anyhow::anyhow!("unknown model"))?;
        let p = self.dir.join(&m.filename);
        if p.exists() {
            std::fs::remove_file(p)?;
        }
        Ok(())
    }

    pub async fn download(self: Arc<Self>, app: AppHandle, id: String) -> anyhow::Result<()> {
        let m = catalog().into_iter().find(|m| m.id == id).ok_or_else(|| anyhow::anyhow!("unknown model"))?;
        if !self.active.lock().insert(id.clone()) {
            return Ok(()); // already downloading
        }
        self.cancel.lock().remove(&id);
        let res = self.clone().download_inner(&app, &m).await;
        self.active.lock().remove(&id);
        let (state, error) = match &res {
            Ok(()) => ("done", None),
            Err(e) if e.to_string() == "cancelled" => ("cancelled", None),
            Err(e) => ("error", Some(e.to_string())),
        };
        let _ = app.emit("model-download", Progress { id: &id, done: 0, total: m.size, state, error });
        res
    }

    async fn download_inner(self: Arc<Self>, app: &AppHandle, m: &CatalogModel) -> anyhow::Result<()> {
        let url = format!("https://huggingface.co/{}/resolve/{}/{}", m.repo, m.revision, m.filename);
        let part = self.dir.join(format!("{}.part", m.filename));
        let dest = self.dir.join(&m.filename);
        let resp = reqwest::Client::new().get(&url).send().await?.error_for_status()?;
        let total = resp.content_length().unwrap_or(m.size);
        let mut file = std::fs::File::create(&part)?;
        let mut hasher = Sha256::new();
        let mut done = 0u64;
        let mut last_emit = 0u64;
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            if self.cancel.lock().remove(&m.id) {
                drop(file);
                let _ = std::fs::remove_file(&part);
                anyhow::bail!("cancelled");
            }
            let chunk = chunk?;
            file.write_all(&chunk)?;
            hasher.update(&chunk);
            done += chunk.len() as u64;
            if done - last_emit > 4_000_000 || done == total {
                last_emit = done;
                let _ = app.emit("model-download", Progress { id: &m.id, done, total, state: "downloading", error: None });
            }
        }
        file.flush()?;
        drop(file);
        let _ = app.emit("model-download", Progress { id: &m.id, done, total, state: "verifying", error: None });
        if let Some(want) = &m.sha256 {
            let got = hex::encode(hasher.finalize());
            if &got != want {
                let _ = std::fs::remove_file(&part);
                anyhow::bail!("Download was corrupted (checksum mismatch). Try again.");
            }
        }
        std::fs::rename(&part, &dest)?;
        Ok(())
    }
}
