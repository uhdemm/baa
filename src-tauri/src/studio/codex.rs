//! Image generation through the Codex CLI's built-in image tool: the same route
//! Sprite Studio uses. Your own ChatGPT/Codex sign-in pays for it, so Baa needs no
//! image API key. The prompt goes in over stdin, never through a shell.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

const NO_WINDOW: u32 = 0x0800_0000;
const TIMEOUT: Duration = Duration::from_secs(12 * 60);

/// How to start Codex: on Windows the npm `codex.cmd` shim is resolved to its
/// codex.js and run with node directly (no cmd.exe parsing of the prompt).
fn codex_command() -> Option<(PathBuf, Vec<String>)> {
    let where_ = |name: &str| -> Option<PathBuf> {
        let mut c = Command::new(if cfg!(windows) { "where.exe" } else { "which" });
        c.arg(name).stdout(Stdio::piped()).stderr(Stdio::null());
        #[cfg(windows)]
        c.creation_flags(NO_WINDOW);
        let out = c.output().ok()?;
        String::from_utf8_lossy(&out.stdout).lines().map(str::trim).find(|l| !l.is_empty()).map(PathBuf::from)
    };
    if cfg!(windows) {
        let shim = where_("codex.cmd")?;
        let js = shim.parent()?.join("node_modules").join("@openai").join("codex").join("bin").join("codex.js");
        let node = where_("node.exe")?;
        js.exists().then(|| (node, vec![js.to_string_lossy().into_owned()]))
    } else {
        where_("codex").map(|p| (p, vec![]))
    }
}

/// A model to run instead of the one in your Codex settings, only when your ChatGPT plan
/// no longer offers that one (the desktop app changes the setting): then the first
/// model the plan lists, from Codex's own cache. None = use your setting.
fn pick_model() -> Option<String> {
    let home = codex_home()?;
    let configured = std::fs::read_to_string(home.join("config.toml")).ok().and_then(|t| {
        t.lines().find_map(|l| l.trim().strip_prefix("model = ").map(|v| v.trim().trim_matches('"').to_string()))
    });
    let text = std::fs::read_to_string(home.join("models_cache.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let mut listed: Vec<(i64, String)> = v["models"]
        .as_array()?
        .iter()
        .filter(|m| m["visibility"].as_str() == Some("list"))
        .filter_map(|m| Some((m["priority"].as_i64().unwrap_or(99), m["slug"].as_str()?.to_string())))
        .collect();
    listed.sort();
    if configured.as_ref().is_some_and(|c| listed.iter().any(|(_, s)| s == c)) {
        return None;
    }
    listed.into_iter().next().map(|(_, slug)| slug)
}

/// Is Codex installed? (Signing in is checked by the first real run.)
pub fn available() -> bool {
    codex_command().is_some()
}

/// Generates one image. `prompt` must name `out` as the save path; `refs` are
/// attached as identity references. `progress` gets short status lines.
pub fn generate(prompt: &str, out: &Path, refs: &[PathBuf], cwd: &Path, progress: impl Fn(&str)) -> anyhow::Result<()> {
    let (cmd, prefix) = codex_command().ok_or_else(|| {
        anyhow::anyhow!("Character Studio uses the Codex CLI to draw. Install it (npm i -g @openai/codex) and sign in once with `codex`.")
    })?;
    let _ = std::fs::remove_file(out);
    let mut args = prefix;
    args.extend(["exec".into(), "--approve-for-me".into(), "--skip-git-repo-check".into()]);
    if let Some(m) = pick_model() {
        args.extend(["-m".into(), m]);
    }
    // the model only has to call the image tool: low thinking saves your Codex usage
    args.extend(["-c".into(), "model_reasoning_effort=\"low\"".into()]);
    for r in refs {
        args.extend(["--image".into(), r.to_string_lossy().into_owned()]);
    }
    args.extend(["-C".into(), cwd.to_string_lossy().into_owned(), "-".into()]);

    let refs_note = if refs.is_empty() {
        String::new()
    } else {
        let list = refs.iter().enumerate().map(|(i, r)| format!("{}. {}", i + 1, r.display())).collect::<Vec<_>>().join("\n");
        format!(
            "\nReference images (also attached via --image):\n{list}\nThese are generation references, not edit targets. If your image tool accepts local image paths \
(referenced_image_paths), pass these so the reference pixels condition the output directly. Do not modify them.\n"
        )
    };
    let instruction = format!(
        "Use the imagegen skill. Built-in image_gen tool path only; do not use the CLI fallback (no OPENAI_API_KEY required).\n\
Save exactly one PNG to the path the request names, then print one line: SAVED: <absolute path>\n\
If the built-in image tool is not available to you, do not draw it any other way: print exactly NO IMAGE TOOL and stop.\n{refs_note}\nRequest:\n\n{prompt}"
    );

    let no_tool = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut c = Command::new(cmd);
    c.args(&args).current_dir(cwd).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    c.creation_flags(NO_WINDOW);
    let mut child = c.spawn()?;
    child.stdin.take().unwrap().write_all(instruction.as_bytes())?; // dropping it closes stdin
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let tail = std::sync::Arc::new(parking_lot::Mutex::new(Vec::<String>::new()));
    let session = std::sync::Arc::new(parking_lot::Mutex::new(None::<String>));
    let (t2, s2) = (tail.clone(), session.clone());
    let err_thread = std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if let Some(id) = line.trim().strip_prefix("session id:") {
                *s2.lock() = Some(id.trim().to_string());
            }
            let mut t = t2.lock();
            t.push(line);
            if t.len() > 30 {
                t.remove(0);
            }
        }
    });
    let started = Instant::now();
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        if let Some(id) = line.trim().strip_prefix("session id:") {
            *session.lock() = Some(id.trim().to_string());
        }
        if line.trim() == "NO IMAGE TOOL" {
            no_tool.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        if line.contains("image_gen") || line.contains("imagegen") {
            progress("Drawing…");
        }
        if started.elapsed() > TIMEOUT {
            let _ = child.kill();
            anyhow::bail!("Codex took too long to draw this one");
        }
    }
    let status = child.wait()?;
    let _ = err_thread.join();
    if out.exists() {
        return Ok(());
    }
    // Codex keeps every picture it draws under ~/.codex/generated_images/<session>/; at
    // low thinking it sometimes doesn't copy it to the path we asked for, so collect it.
    let id = session.lock().clone();
    if let Some(found) = id.and_then(|id| newest_png(&codex_home()?.join("generated_images").join(id))) {
        std::fs::copy(found, out)?;
        return Ok(());
    }
    if no_tool.load(std::sync::atomic::Ordering::SeqCst) {
        anyhow::bail!(
            "Your ChatGPT plan can't draw pictures in Codex right now (its image tool isn't offered to your account). \
Check chatgpt.com/codex/settings/usage, then try again."
        );
    }
    let why = tail.lock().iter().rev().find(|l| l.to_lowercase().contains("error") || l.contains("login")).cloned();
    anyhow::bail!(
        "Codex didn't save a picture ({}){}",
        if status.success() { "it finished without one" } else { "it stopped with an error" },
        why.map(|w| format!(": {w}")).unwrap_or_default()
    )
}

fn codex_home() -> Option<PathBuf> {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(|h| PathBuf::from(h).join(".codex")))
}

/// The last picture drawn in one Codex session (the final try, if it drew several).
fn newest_png(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("png")))
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
        .map(|e| e.path())
}
