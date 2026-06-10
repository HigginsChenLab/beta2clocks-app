import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------
const $ = (id) => document.getElementById(id);
const show = (id) => {
  ["view-engine", "view-setup", "view-run"].forEach((v) =>
    $(v).classList.toggle("hidden", v !== id),
  );
};
const dirname = (p) => (p.includes("/") ? p.slice(0, p.lastIndexOf("/")) : p);
const fmtTime = (s) => {
  s = Math.max(0, Math.round(s));
  const m = Math.floor(s / 60);
  const sec = s % 60;
  if (m >= 60) {
    const h = Math.floor(m / 60);
    return `${h}:${String(m % 60).padStart(2, "0")}:${String(sec).padStart(2, "0")}`;
  }
  return `${m}:${String(sec).padStart(2, "0")}`;
};
const icon = {
  check: `<svg class="w-4 h-4" fill="none" stroke="currentColor" stroke-width="2.5" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg>`,
  x: `<svg class="w-4 h-4" fill="none" stroke="currentColor" stroke-width="2.5" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M6 6l12 12M18 6L6 18"/></svg>`,
  spinner: `<svg class="w-4 h-4 spin" fill="none" stroke="currentColor" stroke-width="2.5" viewBox="0 0 24 24"><path stroke-linecap="round" d="M12 3a9 9 0 109 9" opacity="0.85"/></svg>`,
};

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------
const state = {
  image: "ghcr.io/higginschenlab/beta2clocks:latest",
  inputPath: null,
  inputDir: null,
  outputDir: null,
  preflightOk: false,
  effectiveTotal: null,
  lastCompleted: 0,
  startMs: null,
  timer: null,
};

// ---------------------------------------------------------------------------
// Header pill
// ---------------------------------------------------------------------------
function setPill(kind, text) {
  const colors = {
    ok: ["bg-green-500/15 text-green-300", "bg-green-400"],
    warn: ["bg-amber-500/15 text-amber-300", "bg-amber-400"],
    bad: ["bg-red-500/15 text-red-300", "bg-red-400"],
    idle: ["bg-gray-800 text-gray-400", "bg-gray-600"],
  }[kind];
  $("docker-pill").className =
    "flex items-center gap-2 text-xs font-medium px-2.5 py-1 rounded-full " + colors[0];
  $("docker-dot").className = "w-2 h-2 rounded-full " + colors[1];
  $("docker-pill-text").textContent = text;
}

// ---------------------------------------------------------------------------
// Engine gate
// ---------------------------------------------------------------------------
const DOCKER_URL = "https://www.docker.com/products/docker-desktop/";
const engineCard = (html) => ($("engine-card").innerHTML = html);

async function checkEngine() {
  show("view-engine");
  setPill("idle", "Checking Docker…");
  engineCard(`
    <div class="text-center py-6">
      <div class="text-blue-400 flex justify-center mb-3">${icon.spinner}</div>
      <p class="text-gray-400">Checking your Docker installation…</p>
    </div>`);

  let status;
  try {
    status = await invoke("check_docker");
  } catch (e) {
    status = { installed: false, running: false, message: String(e) };
  }

  if (!status.installed) {
    setPill("bad", "Docker not found");
    engineCard(`
      <div class="text-center">
        <h2 class="text-lg font-semibold text-gray-100">Docker is required</h2>
        <p class="text-gray-400 mt-2 text-sm max-w-md mx-auto">beta2clocks runs the analysis inside a Docker container so your results exactly match the TranslAGE pipeline. Install Docker Desktop (free), then come back.</p>
        <div class="flex items-center justify-center gap-3 mt-6">
          <a href="${DOCKER_URL}" target="_blank" rel="noopener noreferrer" class="bg-blue-600 text-white px-5 py-2.5 rounded-lg font-semibold hover:bg-blue-500 transition-colors">Get Docker Desktop</a>
          <button id="btn-recheck" class="border-2 border-gray-700 text-gray-300 px-5 py-2.5 rounded-lg font-semibold hover:bg-gray-800 transition-colors">Re-check</button>
        </div>
      </div>`);
    $("btn-recheck").onclick = checkEngine;
    return;
  }

  if (!status.running) {
    setPill("warn", "Docker not running");
    engineCard(`
      <div class="text-center">
        <h2 class="text-lg font-semibold text-gray-100">Start Docker Desktop</h2>
        <p class="text-gray-400 mt-2 text-sm max-w-md mx-auto">${status.message || "Docker is installed but the engine isn't running. Open Docker Desktop, wait for it to say “running”, then re-check."}</p>
        <button id="btn-recheck" class="mt-6 bg-blue-600 text-white px-5 py-2.5 rounded-lg font-semibold hover:bg-blue-500 transition-colors">Re-check</button>
      </div>`);
    $("btn-recheck").onclick = checkEngine;
    return;
  }

  setPill("ok", `Docker ${status.version || "ready"}`);

  let hasImage = false;
  try {
    hasImage = await invoke("check_image", { image: state.image });
  } catch (_) {}

  if (!hasImage) {
    engineCard(`
      <div class="text-center">
        <h2 class="text-lg font-semibold text-gray-100">Download the clock engine</h2>
        <p class="text-gray-400 mt-2 text-sm max-w-md mx-auto">One-time download of the beta2clocks engine image (a few GB). This contains R, methylCIPHER and all clock reference data, so everything afterwards runs offline.</p>
        <button id="btn-pull" class="mt-6 bg-gradient-to-r from-blue-600 to-purple-600 text-white px-6 py-2.5 rounded-lg font-semibold hover:from-blue-500 hover:to-purple-500 transition-all">Download engine</button>
        <div id="pull-status" class="hidden mt-5 text-left">
          <div class="flex items-center gap-2 text-sm text-gray-400 mb-2"><span class="text-blue-400">${icon.spinner}</span><span id="pull-line" class="truncate">Starting download…</span></div>
          <div class="relative h-2 rounded-full bg-gray-800 overflow-hidden indeterminate-bar"></div>
        </div>
      </div>`);
    $("btn-pull").onclick = pullImage;
    return;
  }

  goToSetup();
}

async function pullImage() {
  $("btn-pull").disabled = true;
  $("btn-pull").classList.add("opacity-50");
  $("pull-status").classList.remove("hidden");
  const un = await listen("pull-progress", (e) => {
    if (typeof e.payload === "string" && e.payload.trim()) {
      $("pull-line").textContent = e.payload.trim();
    }
  });
  let res;
  try {
    res = await invoke("pull_image", { image: state.image });
  } catch (e) {
    res = { success: false, message: String(e) };
  }
  un();
  if (res.success) {
    goToSetup();
  } else {
    $("pull-line").textContent = res.message;
    $("btn-pull").disabled = false;
    $("btn-pull").classList.remove("opacity-50");
    $("btn-pull").textContent = "Retry download";
  }
}

// ---------------------------------------------------------------------------
// Setup / preflight
// ---------------------------------------------------------------------------
const goToSetup = () => show("view-setup");

async function chooseInput() {
  const selected = await open({
    multiple: false,
    filters: [{ name: "R data", extensions: ["RData", "Rdata", "rdata", "rda"] }],
  });
  if (!selected || typeof selected !== "string") return;
  state.inputPath = selected;
  state.inputDir = dirname(selected);
  state.outputDir = state.inputDir;
  state.preflightOk = false;

  const name = selected.split("/").pop();
  $("btn-choose-input").innerHTML = `
    <svg class="w-7 h-7 mx-auto text-blue-400" fill="none" stroke="currentColor" stroke-width="1.7" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M9 12h6m-6 4h6m2 4H7a2 2 0 01-2-2V6a2 2 0 012-2h7l5 5v9a2 2 0 01-2 2z"/></svg>
    <span class="block mt-2 font-medium text-gray-200 selectable">${name}</span>
    <span class="block text-xs text-blue-400 mt-0.5">Click to choose a different file</span>`;

  runPreflight();
}

function setSetupEnabled(ok) {
  state.preflightOk = ok;
  $("card-output").classList.toggle("opacity-50", !ok);
  $("card-output").classList.toggle("pointer-events-none", !ok);
  $("card-advanced").classList.toggle("opacity-50", !ok);
  $("card-advanced").classList.toggle("pointer-events-none", !ok);
  $("btn-run").disabled = !ok;
  $("run-hint").textContent = ok
    ? "Ready — click Run clocks."
    : "Choose a valid input file to begin.";
  $("output-path").textContent = state.outputDir || "—";
}

async function runPreflight() {
  const box = $("preflight");
  box.classList.remove("hidden");
  box.innerHTML = `<div class="flex items-center gap-2 text-sm text-gray-400 rounded-lg bg-gray-800/60 px-4 py-3"><span class="text-blue-400">${icon.spinner}</span> Spot-checking your data…</div>`;
  setSetupEnabled(false);

  let r;
  try {
    r = await invoke("preflight", { image: state.image, inputPath: state.inputPath });
  } catch (e) {
    box.innerHTML = `<div class="rounded-lg bg-red-500/10 border border-red-500/30 px-4 py-3 text-sm text-red-300"><span class="font-semibold">Couldn't read the file.</span><pre class="selectable mt-1 whitespace-pre-wrap text-xs text-red-400">${String(e)}</pre></div>`;
    setSetupEnabled(false);
    return;
  }
  renderPreflight(r);
}

function renderPreflight(r) {
  const box = $("preflight");
  const checks = (r.checks || [])
    .map((c) => {
      const color = c.pass ? "text-green-400" : "text-red-400";
      const ic = c.pass ? icon.check : icon.x;
      return `<li class="flex items-start gap-2 py-1"><span class="${color} mt-0.5 shrink-0">${ic}</span><span class="text-sm text-gray-400"><span class="font-medium text-gray-200">${c.name}.</span> ${c.message}</span></li>`;
    })
    .join("");

  const stat = (label, val) =>
    `<div><p class="text-xs text-gray-500">${label}</p><p class="text-sm font-semibold text-gray-100">${val}</p></div>`;
  const stats = [
    r.n_samples != null ? stat("Samples", r.n_samples.toLocaleString()) : "",
    r.n_cpgs != null ? stat("CpGs", r.n_cpgs.toLocaleString()) : "",
    r.array_type ? stat("Array", r.array_type) : "",
    r.na_pct != null ? stat("Missing", r.na_pct + "%") : "",
  ].join("");

  const banner = r.ok
    ? `<div class="flex items-center gap-2 rounded-t-lg bg-green-500/10 border border-green-500/30 px-4 py-3 text-green-300"><span>${icon.check}</span><span class="text-sm font-semibold">Looks good — ready to run.</span></div>`
    : `<div class="flex items-center gap-2 rounded-t-lg bg-red-500/10 border border-red-500/30 px-4 py-3 text-red-300"><span>${icon.x}</span><span class="text-sm font-semibold">This file can't be processed yet. See below.</span></div>`;

  box.innerHTML = `
    ${banner}
    <div class="border border-t-0 border-gray-800 rounded-b-lg px-4 py-3">
      ${stats ? `<div class="grid grid-cols-2 sm:grid-cols-4 gap-3 pb-3 mb-2 border-b border-gray-800">${stats}</div>` : ""}
      <ul>${checks}</ul>
    </div>`;

  setSetupEnabled(r.ok);
}

async function chooseOutput() {
  const selected = await open({ directory: true, multiple: false });
  if (selected && typeof selected === "string") {
    state.outputDir = selected;
    $("output-path").textContent = selected;
  }
}

// ---------------------------------------------------------------------------
// Run
// ---------------------------------------------------------------------------
let unlistenLog = null;
let unlistenProg = null;

function resetRunView() {
  $("log").textContent = "";
  $("log-status").textContent = "streaming…";
  $("log-status").className = "text-xs text-blue-400";
  $("run-title").textContent = "Calculating clocks…";
  $("run-subtitle").textContent = "Starting the engine…";
  $("result-hero").classList.add("hidden");
  $("result-clocks").classList.add("hidden");
  $("rep-output-wrap").classList.add("hidden");
  $("progress-fill").style.width = "0%";
  $("progress-pct").textContent = "";
  $("progress-label").textContent = "Preparing…";
  $("progress-track").classList.add("indeterminate-bar");
  ["chip-array", "chip-batch", "chip-current"].forEach((c) => $(c).classList.add("hidden"));
  $("stat-eta-label").textContent = "Est. remaining";
  $("stat-eta").textContent = "—";
  $("stat-clocks").textContent = "0";
  $("stat-samples").textContent = "—";
  $("stat-elapsed").textContent = "0:00";
  // restore the Cancel action
  $("run-actions").innerHTML = `<button id="btn-cancel" class="border-2 border-red-500/60 text-red-400 px-4 py-2 rounded-lg text-sm font-semibold hover:bg-red-500/10 transition-colors">Cancel</button>`;
  $("btn-cancel").onclick = cancelRun;
}

async function startRun() {
  if (!state.preflightOk) return;
  show("view-run");
  resetRunView();
  state.effectiveTotal = null;
  state.lastCompleted = 0;
  state.startMs = Date.now();
  state.timer = setInterval(tickTimer, 1000);
  tickTimer();

  unlistenLog = await listen("run-log", (e) => {
    const log = $("log");
    log.textContent += e.payload + "\n";
    log.scrollTop = log.scrollHeight;
  });
  unlistenProg = await listen("run-progress", (e) => updateProgress(e.payload));

  const options = {
    batchSize: parseBatch(),
    clocks: ($("opt-clocks").value || "").trim() || null,
  };

  let report;
  try {
    report = await invoke("run_clocks", {
      image: state.image,
      inputPath: state.inputPath,
      outputDir: state.outputDir,
      options,
    });
  } catch (e) {
    report = {
      success: false,
      partial: false,
      cancelled: false,
      message: "The run failed to start: " + String(e),
      succeeded: [],
      failed: [],
    };
  }
  finishRun(report);
}

function parseBatch() {
  const v = $("opt-batch").value.trim();
  if (v === "") return null;
  const n = parseInt(v, 10);
  return Number.isFinite(n) ? n : null;
}

function tickTimer() {
  const elapsed = (Date.now() - state.startMs) / 1000;
  $("stat-elapsed").textContent = fmtTime(elapsed);
  if (state.effectiveTotal && state.lastCompleted > 0) {
    const perClock = elapsed / state.lastCompleted;
    const remaining = perClock * (state.effectiveTotal - state.lastCompleted);
    $("stat-eta").textContent = remaining > 0 ? fmtTime(remaining) : "almost done";
  }
}

function updateProgress(p) {
  $("run-subtitle").textContent = "Running the methylCIPHER pipeline…";
  if (p.n_samples != null) $("stat-samples").textContent = p.n_samples.toLocaleString();
  state.lastCompleted = p.completed || 0;
  state.effectiveTotal = p.effective_total || null;

  if (state.effectiveTotal) {
    $("progress-track").classList.remove("indeterminate-bar");
    const pct = Math.min(100, Math.round((p.completed / state.effectiveTotal) * 100));
    $("progress-fill").style.width = pct + "%";
    $("progress-pct").textContent = pct + "%";
    $("progress-label").textContent = `${p.completed} of ${state.effectiveTotal} clocks`;
    $("stat-clocks").textContent = `${p.completed} / ${state.effectiveTotal}`;
  } else {
    $("progress-label").textContent = `${p.completed} clocks done`;
    $("stat-clocks").textContent = String(p.completed);
  }

  if (p.array_type) {
    $("chip-array").textContent = p.array_type + " array";
    $("chip-array").classList.remove("hidden");
  }
  if (p.batch_total && p.batch_total > 1 && p.batch_current) {
    $("chip-batch").textContent = `Batch ${p.batch_current} / ${p.batch_total}`;
    $("chip-batch").classList.remove("hidden");
  }
  if (p.current_clock) {
    $("chip-current").textContent = "Calculating " + p.current_clock;
    $("chip-current").classList.remove("hidden");
  }
}

function cleanupRun() {
  if (state.timer) clearInterval(state.timer);
  state.timer = null;
  if (unlistenLog) unlistenLog();
  if (unlistenProg) unlistenProg();
  unlistenLog = unlistenProg = null;
}

async function cancelRun() {
  const btn = $("btn-cancel");
  if (btn) {
    btn.disabled = true;
    btn.textContent = "Cancelling…";
  }
  try {
    await invoke("cancel_run");
  } catch (_) {}
}

// ---------------------------------------------------------------------------
// Finish — keep the live readout, layer the report on top
// ---------------------------------------------------------------------------
function finishRun(r) {
  cleanupRun();

  // log keeps its content; just mark it stopped
  $("log-status").textContent = r.cancelled ? "stopped" : "finished";
  $("log-status").className = "text-xs text-gray-500";

  // banner tone
  let tone, ic, heroTitle;
  if (r.cancelled) {
    tone = "bg-gray-800 text-gray-200 border border-gray-700";
    ic = `<div class="w-10 h-10 rounded-full bg-gray-600 text-white flex items-center justify-center">${icon.x}</div>`;
    heroTitle = "Run cancelled";
  } else if (r.success && !r.partial) {
    tone = "bg-green-500/10 text-green-300 border border-green-500/30";
    ic = `<div class="w-10 h-10 rounded-full bg-green-500 text-white flex items-center justify-center">${icon.check}</div>`;
    heroTitle = "All clocks calculated";
  } else if (r.partial) {
    tone = "bg-amber-500/10 text-amber-300 border border-amber-500/30";
    ic = `<div class="w-10 h-10 rounded-full bg-amber-500 text-white flex items-center justify-center">${icon.check}</div>`;
    heroTitle = "Finished with some failures";
  } else {
    tone = "bg-red-500/10 text-red-300 border border-red-500/30";
    ic = `<div class="w-10 h-10 rounded-full bg-red-500 text-white flex items-center justify-center">${icon.x}</div>`;
    heroTitle = "Run did not finish";
  }
  const hero = $("result-hero");
  hero.className = "rounded-xl p-5 flex items-center gap-4 " + tone;
  hero.innerHTML = `${ic}<div><p class="text-lg font-bold">${heroTitle}</p><p class="text-sm opacity-80">${r.message || ""}</p></div>`;
  hero.classList.remove("hidden");

  // header
  $("run-title").textContent = r.cancelled ? "Cancelled" : r.success ? "Done" : "Stopped";
  $("run-subtitle").textContent =
    r.total_minutes != null ? `Finished in ${r.total_minutes} min` : r.message || "";

  // progress finalize
  $("progress-track").classList.remove("indeterminate-bar");
  if (r.success && !r.partial) {
    $("progress-fill").style.width = "100%";
    $("progress-pct").textContent = "100%";
    $("progress-label").textContent = "Complete";
  } else {
    $("progress-label").textContent = r.cancelled ? "Stopped" : "Finished";
  }

  // ETA stat → total time
  $("stat-eta-label").textContent = "Total time";
  $("stat-eta").textContent = r.total_minutes != null ? `${r.total_minutes}m` : "—";

  // clock chips + output
  const okList = r.succeeded || [];
  const failList = r.failed || [];
  if (okList.length || failList.length || r.output_path) {
    const failChips = failList.map(
      (n) =>
        `<span class="inline-flex items-center gap-1 px-2.5 py-1 rounded-full bg-red-500/10 text-red-300 text-xs font-medium"><span class="text-red-400">${icon.x}</span>${n}</span>`,
    );
    const okChips = okList.map(
      (n) =>
        `<span class="inline-flex items-center gap-1 px-2.5 py-1 rounded-full bg-green-500/10 text-green-300 text-xs font-medium"><span class="text-green-400">${icon.check}</span>${n}</span>`,
    );
    $("rep-chips").innerHTML =
      failChips.join("") + okChips.join("") ||
      `<span class="text-sm text-gray-500">No clock results were reported.</span>`;
    if (r.output_path) {
      $("rep-output-wrap").classList.remove("hidden");
      $("rep-output").textContent = r.output_path;
      $("btn-open-folder").onclick = () => openPath(dirname(r.output_path));
    }
    $("result-clocks").classList.remove("hidden");
  }

  // actions → run another
  $("run-actions").innerHTML = `<button id="btn-again" class="bg-blue-600 text-white px-4 py-2 rounded-lg text-sm font-semibold hover:bg-blue-500 transition-colors">Run another dataset</button>`;
  $("btn-again").onclick = resetForAnother;
}

function resetForAnother() {
  state.inputPath = null;
  state.preflightOk = false;
  $("preflight").classList.add("hidden");
  $("btn-choose-input").innerHTML = `
    <svg class="w-8 h-8 mx-auto text-gray-500 group-hover:text-blue-400 transition-colors" fill="none" stroke="currentColor" stroke-width="1.7" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M12 16V4m0 0L8 8m4-4l4 4M4 16v2a2 2 0 002 2h12a2 2 0 002-2v-2"/></svg>
    <span class="block mt-2 font-medium text-gray-300 group-hover:text-blue-400">Choose a <code>.RData</code> file</span>
    <span class="block text-xs text-gray-500 mt-0.5">A beta matrix (<code>DNAm…</code>) + phenotypes (<code>pheno…</code>)</span>`;
  setSetupEnabled(false);
  goToSetup();
}

// ---------------------------------------------------------------------------
// Wire up
// ---------------------------------------------------------------------------
$("btn-choose-input").onclick = chooseInput;
$("btn-choose-output").onclick = chooseOutput;
$("btn-run").onclick = startRun;
$("btn-cancel").onclick = cancelRun;

(async function init() {
  try {
    state.image = await invoke("default_image");
  } catch (_) {}
  checkEngine();
})();
