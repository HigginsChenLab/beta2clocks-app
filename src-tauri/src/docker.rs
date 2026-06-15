//! Docker orchestration for the beta2clocks app.
//!
//! The app never bundles R. Instead it drives the published public image
//! `ghcr.io/higginschenlab/beta2clocks:latest`, which already contains R +
//! methylCIPHER + reference data. This module:
//!   * locates the `docker` binary (GUI apps don't inherit the shell PATH),
//!   * checks daemon/image status,
//!   * streams `docker pull` progress,
//!   * runs a base-R preflight script (bundled, mounted in),
//!   * runs the real pipeline and parses its log stream into live progress.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::AppState;

pub const DEFAULT_IMAGE: &str = "ghcr.io/higginschenlab/beta2clocks:latest";

/// The bundled preflight script, baked into the binary at compile time so it
/// works identically in `tauri dev` and in the packaged app.
const PREFLIGHT_R: &str = include_str!("../resources/preflight.R");

// ---------------------------------------------------------------------------
// Docker binary discovery
// ---------------------------------------------------------------------------

/// Locate the `docker` executable. macOS .app bundles launch with a minimal
/// PATH, so we probe the usual install locations before falling back to PATH.
fn docker_path() -> Option<PathBuf> {
    let candidates = [
        "/usr/local/bin/docker",
        "/opt/homebrew/bin/docker",
        "/Applications/Docker.app/Contents/Resources/bin/docker",
        "/usr/bin/docker",
        // Windows default install
        "C:\\Program Files\\Docker\\Docker\\resources\\bin\\docker.exe",
    ];
    for c in candidates {
        let p = Path::new(c);
        if p.exists() {
            return Some(p.to_path_buf());
        }
    }
    // Last resort: rely on PATH (works when launched from a terminal).
    Some(PathBuf::from("docker"))
}

fn docker_cmd() -> Result<Command, String> {
    let path = docker_path().ok_or_else(|| "docker binary not found".to_string())?;
    Ok(Command::new(path))
}

// ---------------------------------------------------------------------------
// Setup: write the preflight script to a mountable cache dir
// ---------------------------------------------------------------------------

/// Write `preflight.R` to the app cache dir (under the user's home, so Docker
/// Desktop can bind-mount it) and return the containing directory.
pub fn init_preflight_script(app: &AppHandle) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let dir = app.path().app_cache_dir()?;
    std::fs::create_dir_all(&dir)?;
    let script = dir.join("preflight.R");
    std::fs::write(&script, PREFLIGHT_R)?;
    Ok(dir)
}

// ---------------------------------------------------------------------------
// check_docker / check_image
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone)]
pub struct DockerStatus {
    pub installed: bool,
    pub running: bool,
    pub version: Option<String>,
    pub message: String,
}

#[tauri::command]
pub fn default_image() -> String {
    DEFAULT_IMAGE.to_string()
}

#[tauri::command]
pub async fn check_docker() -> Result<DockerStatus, String> {
    let path = match docker_path() {
        Some(p) => p,
        None => {
            return Ok(DockerStatus {
                installed: false,
                running: false,
                version: None,
                message: "Docker is not installed.".into(),
            })
        }
    };
    // Treat the bare "docker" fallback as "not installed" unless it actually runs.
    let installed = path != PathBuf::from("docker") || which_docker_runs(&path).await;

    let out = Command::new(&path)
        .args(["version", "--format", "{{.Server.Version}}"])
        .output()
        .await;

    match out {
        Ok(o) if o.status.success() => {
            let v = String::from_utf8_lossy(&o.stdout).trim().to_string();
            Ok(DockerStatus {
                installed: true,
                running: true,
                version: if v.is_empty() { None } else { Some(v) },
                message: "Docker is running.".into(),
            })
        }
        Ok(o) => {
            // Binary exists but daemon unreachable.
            let err = String::from_utf8_lossy(&o.stderr).to_string();
            Ok(DockerStatus {
                installed,
                running: false,
                version: None,
                message: if err.trim().is_empty() {
                    "Docker is installed but not running. Start Docker Desktop and try again.".into()
                } else {
                    format!("Docker is not running: {}", err.trim())
                },
            })
        }
        Err(_) => Ok(DockerStatus {
            installed: false,
            running: false,
            version: None,
            message: "Docker is not installed.".into(),
        }),
    }
}

async fn which_docker_runs(path: &Path) -> bool {
    Command::new(path)
        .arg("--version")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[tauri::command]
pub async fn check_image(image: String) -> Result<bool, String> {
    let mut cmd = docker_cmd()?;
    let out = cmd
        .args(["image", "inspect", &image])
        .output()
        .await
        .map_err(|e| e.to_string())?;
    Ok(out.status.success())
}

// ---------------------------------------------------------------------------
// pull_image (streamed)
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone)]
pub struct PullResult {
    pub success: bool,
    pub message: String,
}

#[tauri::command]
pub async fn pull_image(app: AppHandle, image: String) -> Result<PullResult, String> {
    let mut cmd = docker_cmd()?;
    let mut child = cmd
        .args(["pull", &image])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start docker pull: {e}"))?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let app1 = app.clone();
    let t1 = tauri::async_runtime::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = app1.emit("pull-progress", line);
        }
    });
    let app2 = app.clone();
    let t2 = tauri::async_runtime::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = app2.emit("pull-progress", line);
        }
    });

    let status = child.wait().await.map_err(|e| e.to_string())?;
    let _ = t1.await;
    let _ = t2.await;

    if status.success() {
        Ok(PullResult {
            success: true,
            message: "Engine downloaded successfully.".into(),
        })
    } else {
        Ok(PullResult {
            success: false,
            message: "Failed to download the engine image. Check your internet connection and try again.".into(),
        })
    }
}

// ---------------------------------------------------------------------------
// preflight
// ---------------------------------------------------------------------------

fn parent_and_name(input_path: &str) -> Result<(PathBuf, String), String> {
    let p = Path::new(input_path);
    let dir = p
        .parent()
        .ok_or_else(|| "Could not determine the folder of the input file.".to_string())?
        .to_path_buf();
    let name = p
        .file_name()
        .ok_or_else(|| "Invalid input file name.".to_string())?
        .to_string_lossy()
        .to_string();
    Ok((dir, name))
}

#[tauri::command]
pub async fn preflight(
    state: State<'_, AppState>,
    image: String,
    input_path: String,
) -> Result<serde_json::Value, String> {
    let (input_dir, file_name) = parent_and_name(&input_path)?;
    let script_dir = state.script_dir.clone();

    let input_mount = format!("{}:/home/data:ro", input_dir.to_string_lossy());
    let script_mount = format!("{}:/home/app:ro", script_dir.to_string_lossy());
    let data_arg = format!("data/{}", file_name);

    let mut cmd = docker_cmd()?;
    let out = cmd
        .args([
            "run",
            "--rm",
            "-v",
            &input_mount,
            "-v",
            &script_mount,
            &image,
            "Rscript",
            "/home/app/preflight.R",
            "--input",
            &data_arg,
        ])
        .output()
        .await
        .map_err(|e| format!("Failed to run preflight: {e}"))?;

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    // The script prints exactly one line beginning with PREFLIGHT_JSON=.
    for line in stdout.lines().chain(stderr.lines()) {
        if let Some(rest) = line.strip_prefix("PREFLIGHT_JSON=") {
            return serde_json::from_str::<serde_json::Value>(rest.trim())
                .map_err(|e| format!("Could not parse preflight result: {e}"));
        }
    }

    // No JSON line: surface the container error.
    let tail: String = stderr
        .lines()
        .rev()
        .take(6)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    Err(format!(
        "Preflight could not read the file. The container reported:\n{}",
        if tail.trim().is_empty() { "(no output)" } else { &tail }
    ))
}

// ---------------------------------------------------------------------------
// run_clocks (streamed, parsed)
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone, Default)]
pub struct RunState {
    pub n_samples: Option<u64>,
    pub n_cpgs: Option<u64>,
    pub array_type: Option<String>,
    pub clocks_per_batch: Option<u64>,
    pub skipped: u64,
    pub batch_current: Option<u64>,
    pub batch_total: Option<u64>,
    pub completed: u64,
    pub current_clock: Option<String>,
    pub succeeded: Vec<String>,
    pub failed: Vec<String>,
    pub total_minutes: Option<f64>,
    /// container-internal output path, captured from "Results saved to:".
    pub output_path_internal: Option<String>,
    /// effective total = (clocks_per_batch - skipped) * batch_total, when known.
    pub effective_total: Option<u64>,
}

#[derive(Serialize, Clone)]
pub struct RunReport {
    pub success: bool,
    pub partial: bool,
    pub cancelled: bool,
    pub exit_code: Option<i32>,
    pub output_path: Option<String>,
    pub total_minutes: Option<f64>,
    pub n_samples: Option<u64>,
    pub array_type: Option<String>,
    pub succeeded: Vec<String>,
    pub failed: Vec<String>,
    pub message: String,
}

fn array_type_from_cpgs(m: u64) -> String {
    if m < 500_000 {
        "450K".into()
    } else if m <= 900_000 {
        "EPIC v1 (850K)".into()
    } else {
        "EPIC v2 (935K)".into()
    }
}

fn between<'a>(s: &'a str, a: &str, b: &str) -> Option<&'a str> {
    let start = s.find(a)? + a.len();
    let rest = &s[start..];
    let end = rest.find(b)?;
    Some(&rest[..end])
}

fn after<'a>(s: &'a str, a: &str) -> Option<&'a str> {
    let i = s.find(a)? + a.len();
    Some(&s[i..])
}

fn recompute_effective_total(st: &mut RunState) {
    if let (Some(per), Some(bt)) = (st.clocks_per_batch, st.batch_total) {
        let eff_per = per.saturating_sub(st.skipped.min(per));
        st.effective_total = Some(eff_per.saturating_mul(bt));
    } else if let Some(per) = st.clocks_per_batch {
        // No batching observed yet → assume single batch.
        let eff_per = per.saturating_sub(st.skipped.min(per));
        st.effective_total = Some(eff_per);
    }
}

/// Parse one log line, mutating state. Returns true if anything changed.
fn parse_line(line: &str, st: &mut RunState) -> bool {
    let l = line.trim();
    let mut changed = false;

    // "  Beta matrix: <var> (N samples x M CpGs)"
    if l.contains("Beta matrix:") && l.contains("samples x") {
        if let Some(n) = between(l, "(", " samples x") {
            if let Ok(v) = n.trim().parse::<u64>() {
                st.n_samples = Some(v);
                changed = true;
            }
        }
        if let Some(m) = between(l, "samples x ", " CpGs") {
            if let Ok(v) = m.trim().parse::<u64>() {
                st.n_cpgs = Some(v);
                st.array_type = Some(array_type_from_cpgs(v));
                changed = true;
            }
        }
    }

    // "Batching: N samples into K batches ..."
    if l.starts_with("Batching:") {
        if let Some(k) = between(l, "into ", " batches") {
            if let Ok(v) = k.trim().parse::<u64>() {
                st.batch_total = Some(v);
                recompute_effective_total(st);
                changed = true;
            }
        }
    }

    // "Batch b/n (samples ...)"
    if l.starts_with("Batch ") && l.contains('/') {
        let frag = after(l, "Batch ").unwrap_or("");
        let frag = frag.split_whitespace().next().unwrap_or("");
        let mut it = frag.split('/');
        if let (Some(b), Some(n)) = (it.next(), it.next()) {
            if let (Ok(b), Ok(n)) = (b.parse::<u64>(), n.parse::<u64>()) {
                st.batch_current = Some(b);
                st.batch_total = Some(n);
                recompute_effective_total(st);
                changed = true;
            }
        }
    }

    // "  Dispatching N clock(s) from metadata."  (per batch; set once)
    if l.contains("Dispatching") && l.contains("from metadata") {
        if let Some(n) = between(l, "Dispatching ", " clock") {
            if let Ok(v) = n.trim().parse::<u64>() {
                if st.clocks_per_batch.is_none() {
                    // +2 for WhatSex + Zhang2019 setup clocks that also dispatch.
                    st.clocks_per_batch = Some(v + 2);
                    recompute_effective_total(st);
                    changed = true;
                }
            }
        }
    }

    // "  Calculating X..."
    if let Some(rest) = after(l, "Calculating ") {
        let name = rest.trim_end_matches('.').trim();
        if !name.is_empty() {
            st.current_clock = Some(name.to_string());
            changed = true;
        }
    }

    // "  X done. (1.2s)"
    if l.contains(" done. (") {
        if let Some(name) = l.split(" done. (").next() {
            let name = name.trim().to_string();
            if !name.is_empty() {
                st.completed += 1;
                if !st.succeeded.contains(&name) {
                    st.succeeded.push(name);
                }
                changed = true;
            }
        }
    }

    // "  Skipping X (...)"
    if l.starts_with("Skipping ") {
        st.skipped += 1;
        recompute_effective_total(st);
        changed = true;
    }

    // "  FAILED clocks (n): a, b, c"
    if l.contains("FAILED clocks (") {
        if let Some(list) = after(l, "): ") {
            st.failed = list
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            changed = true;
        }
    }

    // "Results saved to: /home/data/DNAmAge....RData"
    if let Some(p) = after(l, "Results saved to: ") {
        st.output_path_internal = Some(p.trim().to_string());
        changed = true;
    }

    // "beta2clocks completed in 3.21 minutes."
    if l.contains("completed in") && l.contains("minutes") {
        if let Some(m) = between(l, "completed in ", " minutes") {
            if let Ok(v) = m.trim().parse::<f64>() {
                st.total_minutes = Some(v);
                changed = true;
            }
        }
    }

    changed
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunOptions {
    pub batch_size: Option<i64>,
    pub clocks: Option<String>,
}

#[tauri::command]
pub async fn run_clocks(
    app: AppHandle,
    state: State<'_, AppState>,
    image: String,
    input_path: String,
    output_dir: Option<String>,
    options: Option<RunOptions>,
) -> Result<RunReport, String> {
    let (input_dir, file_name) = parent_and_name(&input_path)?;
    let opts = options.unwrap_or(RunOptions {
        batch_size: None,
        clocks: None,
    });

    // Unique container name for cancellation.
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let container_name = format!("beta2clocks-run-{nanos}");
    *state.run_container.lock().unwrap() = Some(container_name.clone());

    // Never bind-mount the user's input folder for writing. Cloud-synced
    // folders (OneDrive/iCloud/Dropbox) deadlock when Docker writes the result
    // back through the mount ("Resource deadlock avoided"). Instead, stage the
    // input into a local scratch dir under the app cache (known to be
    // Docker-shareable — preflight mounts it), mount THAT read-write, and copy
    // the result out to the host afterward. Native writes to cloud folders are
    // fine; only Docker's mount writes are not.
    let scratch = state.script_dir.join(format!("run-{nanos}"));
    std::fs::create_dir_all(&scratch)
        .map_err(|e| format!("Could not create a working folder: {e}"))?;
    let _scratch_guard = ScratchGuard(scratch.clone());
    stage_input(Path::new(&input_path), &scratch.join(&file_name))
        .map_err(|e| format!("Could not prepare the input file for the run: {e}"))?;

    let data_mount = format!("{}:/home/data:rw", scratch.to_string_lossy());
    let data_arg = format!("data/{}", file_name);

    let mut args: Vec<String> = vec![
        "run".into(),
        "--rm".into(),
        "--name".into(),
        container_name.clone(),
        "-v".into(),
        data_mount,
        image.clone(),
        "Rscript".into(),
        "pipeline/entrypoint.R".into(),
        "--input".into(),
        data_arg,
    ];
    if let Some(bs) = opts.batch_size {
        args.push("--batch-size".into());
        args.push(bs.to_string());
    }
    if let Some(cl) = opts.clocks.as_ref().filter(|s| !s.trim().is_empty()) {
        args.push("--clocks".into());
        args.push(cl.trim().to_string());
    }

    let mut cmd = docker_cmd()?;
    let mut child = cmd
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start the run: {e}"))?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let st = Arc::new(Mutex::new(RunState::default()));

    // R's message() writes to stderr; ordinary output to stdout. Read both.
    let st1 = st.clone();
    let app1 = app.clone();
    let t1 = tauri::async_runtime::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = app1.emit("run-log", &line);
            let mut s = st1.lock().unwrap();
            if parse_line(&line, &mut s) {
                let _ = app1.emit("run-progress", s.clone());
            }
        }
    });
    let st2 = st.clone();
    let app2 = app.clone();
    let t2 = tauri::async_runtime::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = app2.emit("run-log", &line);
            let mut s = st2.lock().unwrap();
            if parse_line(&line, &mut s) {
                let _ = app2.emit("run-progress", s.clone());
            }
        }
    });

    let status = child.wait().await.map_err(|e| e.to_string())?;
    let _ = t1.await;
    let _ = t2.await;

    // Clear the active container handle.
    let was_cancelled = {
        let mut guard = state.run_container.lock().unwrap();
        let cancelled = guard.is_none(); // cancel_run sets it to None after killing
        *guard = None;
        cancelled
    };

    let code = status.code();
    let final_state = st.lock().unwrap().clone();

    // SIGKILL from `docker kill` surfaces as 137; treat as cancellation.
    let cancelled = was_cancelled || code == Some(137);

    // The result is written in the local scratch dir; copy it out to the
    // user's chosen folder (or back beside the input if none was chosen).
    let mut output_path: Option<String> = None;
    if code == Some(0) || code == Some(2) {
        let dataset = dataset_name_from(&file_name);
        let produced = scratch.join(format!("DNAmAge{dataset}.RData"));
        if produced.exists() {
            let dest_dir = output_dir
                .as_ref()
                .map(PathBuf::from)
                .unwrap_or_else(|| input_dir.clone());
            let dest = dest_dir.join(produced.file_name().unwrap());
            match move_file(&produced, &dest) {
                Ok(_) => output_path = Some(dest.to_string_lossy().to_string()),
                Err(e) => {
                    let _ = app.emit(
                        "run-log",
                        format!(
                            "Note: could not save the result to {}: {e}",
                            dest_dir.to_string_lossy()
                        ),
                    );
                }
            }
        }
    }

    let (success, partial, message) = if cancelled {
        (false, false, "Run cancelled.".to_string())
    } else {
        match code {
            Some(0) => (true, false, "All clocks completed successfully.".to_string()),
            Some(2) => (
                true,
                true,
                format!(
                    "Completed, but {} clock(s) failed.",
                    final_state.failed.len().max(1)
                ),
            ),
            Some(1) => (
                false,
                false,
                "The pipeline hit a fatal error and stopped. See the log for details.".to_string(),
            ),
            other => (
                false,
                false,
                format!("The run ended unexpectedly (exit code {:?}).", other),
            ),
        }
    };

    Ok(RunReport {
        success,
        partial,
        cancelled,
        exit_code: code,
        output_path,
        total_minutes: final_state.total_minutes,
        n_samples: final_state.n_samples,
        array_type: final_state.array_type,
        succeeded: final_state.succeeded,
        failed: final_state.failed,
        message,
    })
}

/// Mirror entrypoint.R: strip extension, then a trailing "_cleaned".
fn dataset_name_from(file_name: &str) -> String {
    let stem = Path::new(file_name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| file_name.to_string());
    stem.strip_suffix("_cleaned").unwrap_or(&stem).to_string()
}

/// Stage the user's input into the local scratch dir so the container never
/// writes through a bind mount of a (possibly cloud-synced) input folder.
/// Hard-link when possible (instant); fall back to a full copy across
/// filesystems or cloud providers that disallow hard links.
fn stage_input(src: &Path, dst: &Path) -> std::io::Result<()> {
    let _ = std::fs::remove_file(dst);
    if std::fs::hard_link(src, dst).is_ok() {
        return Ok(());
    }
    std::fs::copy(src, dst)?;
    Ok(())
}

/// Removes the per-run scratch dir when the run finishes, however it exits.
struct ScratchGuard(PathBuf);
impl Drop for ScratchGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Rename, falling back to copy+remove across filesystems/mounts.
fn move_file(src: &Path, dest: &Path) -> std::io::Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::rename(src, dest) {
        Ok(_) => Ok(()),
        Err(_) => {
            std::fs::copy(src, dest)?;
            std::fs::remove_file(src)?;
            Ok(())
        }
    }
}

#[tauri::command]
pub async fn cancel_run(state: State<'_, AppState>) -> Result<(), String> {
    let name = {
        let mut guard = state.run_container.lock().unwrap();
        guard.take()
    };
    if let Some(name) = name {
        let mut cmd = docker_cmd()?;
        let _ = cmd.args(["kill", &name]).output().await;
    }
    Ok(())
}
