# beta2clocks — Desktop App

A clean desktop app for calculating epigenetic clocks from DNA methylation data,
built on the [TranslAGE](https://translage.io) / methylCIPHER pipeline. It wraps the
public [`beta2clocks`](https://github.com/HigginsChenLab/beta2clocks) Docker image in a
friendly UI — choose an `.RData` file, let it spot-check the data, click **Run clocks**,
and get a `DNAmAge….RData` of results with a per-clock report.

No command line. No R installation. Your results exactly match the published TranslAGE
pipeline because the same container does the math.

## How it works

```
 ┌────────────┐   docker run (preflight.R)    ┌─────────────────────────────┐
 │  Tauri app │ ────────────────────────────▶ │  ghcr.io/higginschenlab/    │
 │ (Rust + JS)│ ◀──── PREFLIGHT_JSON ───────── │  beta2clocks:latest         │
 │            │                                │  (R + methylCIPHER + data)  │
 │            │   docker run (entrypoint.R)    │                             │
 │            │ ────────────────────────────▶ │                             │
 │            │ ◀──── streamed log lines ───── │                             │
 └────────────┘   parsed → live progress       └─────────────────────────────┘
```

* **No container changes.** The app drives the stock published image. Preflight mounts a
  small bundled `preflight.R` (base R only) into the image; the run uses the image's own
  `pipeline/entrypoint.R` and parses its log stream for live progress.
* **Engine, not bundled R.** The clock math (heavy Bioconductor packages + ~GB reference
  data) lives in the Docker image, downloaded once on first launch.

## Requirements

* [Docker Desktop](https://www.docker.com/products/docker-desktop/) (free), running.
* macOS Apple Silicon for the prebuilt release (Windows/Intel/Linux planned).

## Input format

An `.RData` file containing:

* a beta matrix named `datMeth` or starting with `DNAm` — samples as **rows**, CpGs as
  **columns**, values in `[0,1]`;
* a phenotype table named `datPheno` or starting with `pheno`, with `cAGE` and `cFEMALE`
  (0 = male, 1 = female). `Age`/`Female` are accepted as fallbacks.

Methylation rows and phenotype rows must be in the **same order**.

### Required CpGs

The clocks the app computes (the public methylCIPHER set) draw on a combined
**134,131 unique CpGs**. The full list lives in
[`required_CpGs.csv`](required_CpGs.csv) — one row per CpG, two columns:

| Column   | Meaning |
|----------|---------|
| `CpG`    | Illumina probe ID (`cg…`/`ch…`) |
| `Clocks` | semicolon-separated list of every clock that requires it |

The two largest panels by far are **SystemsAge** (125,175 CpGs) and **PCClocks**
(78,464); every other public clock contributes its own required probes too. Many
probes are shared — 80,694 of the 134,131 are used by more than one clock.

You don't need all of these present to get results — missing CpGs are
mean-imputed and each clock falls back to its author-intended missing-probe
handling — but a dataset covering more of this list yields more reliable scores.
Use the CSV to check coverage before a run, or to subset an array manifest.

> The list is generated from the bundled `*_CpGs` data objects in
> [methylCIPHER](https://github.com/HigginsChenLab/methylCIPHER) (plus the
> `whatsex`, `DunedinPACE`, and `EpiDISH` reference panels). Regenerate it when
> the clock set changes.

## Develop

```bash
npm install
npm run tauri dev        # launches the app with hot-reload
```

Generate test fixtures (needs a local R) and exercise the preflight logic directly:

```bash
Rscript test/make_fixtures.R test/fixtures
Rscript src-tauri/resources/preflight.R --input test/fixtures/valid_cleaned.RData
```

## Build a release

```bash
npm run tauri build      # → src-tauri/target/release/bundle/macos/beta2clocks.app
./scripts/make-dmg.sh    # → src-tauri/target/release/bundle/dmg/beta2clocks_<ver>_<arch>.dmg
```

The bundle target is `app` only — the DMG is built separately by `scripts/make-dmg.sh`
(via `hdiutil`), because Tauri's default DMG step drives Finder/AppleScript and fails in
headless/CI sessions.

The macOS build is not yet notarized; on first open you may need to right-click →
**Open**, or run `xattr -dr com.apple.quarantine /Applications/beta2clocks.app`.

## Releasing (multi-OS via GitHub Actions)

`.github/workflows/release.yml` builds native installers for macOS (Apple
Silicon + Intel), Windows, and Linux in parallel and uploads them to a GitHub
Release. To cut a release:

```bash
# bump version in package.json + src-tauri/tauri.conf.json + src-tauri/Cargo.toml, then:
git tag v0.1.0 && git push origin v0.1.0
```

The workflow creates a **draft** Release with all installers attached; review
and publish it. The website links to `releases/latest/download/…`, so nothing
needs to be hosted on the site.

### Code signing & notarization (optional but recommended)

Without signing, downloaded builds hit Gatekeeper ("damaged"/unidentified on
macOS, SmartScreen on Windows). To produce signed, notarized macOS builds, add
these repo secrets (Settings → Secrets and variables → Actions). Requires an
Apple Developer account.

| Secret | What it is |
|---|---|
| `APPLE_CERTIFICATE` | Your **Developer ID Application** cert exported from Keychain as `.p12`, then base64-encoded (`base64 -i cert.p12 \| pbcopy`). |
| `APPLE_CERTIFICATE_PASSWORD` | The password you set when exporting the `.p12`. |
| `APPLE_SIGNING_IDENTITY` | `Developer ID Application: Your Name (TEAMID)` — the exact cert name. |
| `APPLE_TEAM_ID` | Your 10-char Apple Team ID. |
| `APPLE_ID` + `APPLE_PASSWORD` | Your Apple ID email + an [app-specific password](https://appleid.apple.com) for notarization. *(Or use App Store Connect API key vars `APPLE_API_ISSUER` / `APPLE_API_KEY` / `APPLE_API_KEY_PATH` instead.)* |

The workflow already wires these env vars; once the secrets exist, signing +
notarization happen automatically.

## Layout

```
src/                     frontend (vanilla JS + Tailwind v4 via Vite)
src-tauri/
  src/docker.rs          Docker orchestration, log parsing, events
  src/lib.rs             app setup + command registration
  resources/preflight.R  base-R spot check, mounted into the image
  tauri.conf.json        window + bundle config
```

## Credits

Built by the Higgins-Chen Lab. Clock implementations from
[methylCIPHER](https://github.com/HigginsChenLab/methylCIPHER).
