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
Save exactly one PNG to the path the request names, then print one line: SAVED: <absolute path>\n{refs_note}\nRequest:\n\n{prompt}"
    );

    let mut c = Command::new(cmd);
    c.args(&args).current_dir(cwd).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    c.creation_flags(NO_WINDOW);
    let mut child = c.spawn()?;
    child.stdin.take().unwrap().write_all(instruction.as_bytes())?; // dropping it closes stdin
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let tail = std::sync::Arc::new(parking_lot::Mutex::new(Vec::<String>::new()));
    let t2 = tail.clone();
    let err_thread = std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            let mut t = t2.lock();
            t.push(line);
            if t.len() > 30 {
                t.remove(0);
            }
        }
    });
    let started = Instant::now();
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
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
    let why = tail.lock().iter().rev().find(|l| l.to_lowercase().contains("error") || l.contains("login")).cloned();
    anyhow::bail!(
        "Codex didn't save a picture ({}){}",
        if status.success() { "it finished without one" } else { "it stopped with an error" },
        why.map(|w| format!(": {w}")).unwrap_or_default()
    )
}
