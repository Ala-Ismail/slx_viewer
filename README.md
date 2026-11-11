# 1. SLX Viewer

## 1.1 Overview

This project is a distilled implementation of an SLX viewer. It is an independent project and is not affiliated with MATLAB or MathWorks.

SLX Viewer is a Windows x64 desktop app for viewing saved Simulink `.slx` models. You can open a model offline without MATLAB or Simulink. The loader opens the input read-only and preserves stored expressions as text.


## 1.2 Build on Windows

To compile from source, install Rust and Cargo for the `x86_64-pc-windows-msvc` target and the Microsoft C++ build tools with a Windows SDK. Run the commands below in PowerShell from the repository root.

```powershell
cargo build --release --locked
```

The desktop build produces `target\release\slx_viewer_desktop.exe`. You can copy the executable to another folder and launch it there. It uses the Windows OpenGL graphics driver and links the C runtime into the executable. Cargo uses the dependency versions in `Cargo.lock`. If those dependencies are already cached, you can build offline with `cargo build --release --locked --offline`.

If a running viewer has the release executable locked, use `cargo build --release --locked --target-dir target/stateflow` to build the binaries under `target\stateflow\release`.

The tested navigation and status-bar layout build is available at `target\release\slx_viewer_desktop.exe`.

### 1.2.1 Dependency notices

Dependency and font notices are embedded in the executable and shown under **Help > License…**. After updating `Cargo.lock`, regenerate them with `python tools/update_notices.py` before building. The generator uses cached dependency sources and the upstream license copies in `notices`.

### 1.2.2 Download a GitHub build

The **Windows build** workflow runs on pushes to `main`, pull requests, version tags (`v*`), and manual dispatch. It checks formatting, Clippy, and tests before building Windows x64 release executables with the locked dependencies.

Open the repository's **Actions > Windows build**, select a successful run, and download **slx_viewer-windows-x64** under **Artifacts**. Extract the downloaded artifact, then extract `slx_viewer-windows-x64.zip` and launch `slx_viewer_desktop.exe`. The package also contains the command-line executable, README, license, third-party notices, and a separate ZIP checksum. Artifacts are retained for 14 days. GitHub sign-in and access to this private repository are required.

For a persistent versioned download, push a version tag after updating the Cargo package version and lockfile as appropriate:

```powershell
git tag v0.1.0
git push origin v0.1.0
```

A successful tagged build creates a GitHub Release with the ZIP and SHA-256 checksum under **Releases > Assets**. Downloads remain restricted while the repository is private. No personal access token secret is needed; the release job uses GitHub's automatic token with permission to write repository contents. The workflow does not perform native GUI or clean-machine acceptance testing.

## 1.3 Open a model

Double-click `slx_viewer_desktop.exe`. The app opens to an empty window. Choose **File > Open…** or press **Ctrl+O**, then select a local `.slx` file. To try the viewer, select `tests\fixtures\dc_motor_speed_control.slx` from the repository. You can also drop a model on the window, or provide a path when launching the app.

```powershell
.\target\release\slx_viewer_desktop.exe
.\target\release\slx_viewer_desktop.exe "C:\Models\My Model.slx"
```

Use **Fit**, **−**, the zoom percentage, and **+** at the bottom right to control the current diagram. Clicking the percentage resets zoom to 100%. **Ctrl+wheel** zooms around the pointer. Long status messages truncate to keep these controls visible; hover over the message to read it in full.

Pan with the middle mouse button or by dragging empty canvas space. Click a block to inspect its saved properties. Open a subsystem from the hierarchy or by double-clicking the block. The **↑** toolbar button, **Esc**, or **Alt+Up** opens the parent. The **←** button or **Alt+Left** returns to the previously viewed system. Hover over either icon for its name and shortcut.

### 1.3.1 Command-line summary

Use `slx_viewer.exe` to print a summary in the terminal. Pass one local SLX path and quote paths containing spaces.

```powershell
.\target\release\slx_viewer.exe tests/fixtures/dc_motor_speed_control.slx
```

The fixture produces this summary.

```text
dc_motor_speed_control
Saved release: R2025b
Systems: 1
  dc_motor_speed_control: 25 blocks, 22 signals, 0 annotations [simulink/systems/system_root.xml]
```

The CLI prints recoverable diagnostics to standard error and still returns the parsed summary. A fatal load error prints an error message and returns a nonzero exit code. From a Rust program, call `slx_viewer::load(path, &slx_viewer::Limits::default())` to access the parsed model and diagnostics.

When a model contains supported Stateflow charts, the CLI also lists their block paths, chart IDs, and object counts. Rust callers can use `Block::chart` to index `Model::charts`.

### 1.3.2 View a Stateflow chart

Open a supported chart by double-clicking its block, pressing Enter with the block selected, or choosing it in the hierarchy. Select chart objects to inspect stored properties. Esc returns to the owning system; Back follows viewing history.

### 1.3.3 Menus and settings

**File > Open Recent** lists the ten most recently opened models, with their paths. Successful opens update the list. Settings are stored in `%APPDATA%\slx_viewer\settings.json`, including the default dark theme preference. Set `SLX_VIEWER_SETTINGS_DIR` to use another settings directory.

**Help > Current Version and Build Information…** shows the version, UTC build timestamp, source revision when available, target, profile, and graphics backend. **Help > License…** shows the project license status and the bundled dependency and font notices offline. The project is licensed under the [MIT License](LICENSE). Bundled dependencies and model fixtures retain their own license terms.

Use **Alt+F** or **Alt+H**, arrow keys, and **Enter** to operate the menus. **Esc** closes the active submenu or dialog before navigating the model.

## 1.4 Supported layouts and limits

- The loader discovers the block diagram and release metadata through package relationships, with fallbacks to the conventional block diagram and release metadata part names. Systems can be embedded in the diagram or resolved through the owning part's system relationships.
- The model retains system-local block identifiers, direct stored properties, saved block bounds, orientation and mirroring flags, declared port counts, ports observed in connections, annotations, and signal branch offsets. The canvas estimates port positions and the last stretch of each wire from these data. Specialized block icons and model-wide search are pending.
- Missing child parts and unresolved endpoints produce diagnostics. External models, model workspaces, defaults, specialized block semantics, and nested parameter structures are outside the current parser coverage. Only the fixtures listed in the [fixture manifest](tests/fixtures/README.md) have been exercised.
- The loader opens the file read-only and processes selected ZIP parts in memory. Defaults cap the compressed file at 128 MiB, the indexed entry count at 4,096, declared and consumed expanded data at 128 MiB, a read part at 16 MiB, and XML at 96 levels and 500,000 elements. System hierarchy has a separate depth cap of 32. The ZIP dependency builds its index before the entry-count check and coalesces duplicate entry names, which limits what that check covers.

## 1.5 Verification

Run these checks from the repository root. Formatting and Clippy use the `rustfmt` and `clippy` Rust components.

```powershell
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
cargo build --release --locked
```

Build `0.1.0`, Rust 1.99.1, `x86_64-pc-windows-msvc`, Windows build 26300. Cargo used cached dependencies and the included `Cargo.lock`. The current documentation cleanup has no new native visual verification.

| Check | Result | Evidence |
| --- | --- | --- |
| Rust formatting | Pass | `cargo fmt --all -- --check` |
| Clippy | Pass | All targets and features, warnings treated as errors |
| Automated tests | Pass | Eleven integration tests and twelve desktop tests across all targets and features, using the retained motor fixture and synthetic packages |
| Locked Windows x64 release build | Pass | Built the CLI and desktop executables under `target\stateflow\release` |
| Native launch and file picker | Pass | On an isolated Windows desktop, File > Open selected a fixture through the native dialog. After restart, Open Recent reopened the saved path. Settings retained two unique paths and the dark preference |
| T02 parser content | Pass for listed fixtures | Independently inspected saved XML and authored synthetic expectations cover counts, identity, hierarchy, properties, port information, and connectivity |
| T01, T03–T04, T08, T10, T12–T13, T15–T16, T18 | Not run in full | Native smoke and automated checks cover picker and launch paths, branch geometry, pointer-anchored zoom limits, parent and Back state, unavailable interiors, read-only file access, malformed packages, and stale or cancelled completion handling |
| T05–T07, T09, T11, T14, T17 | Not run in full | Manual canvas interactions, full keyboard priority, model-wide search, process and network observation, and clean-machine distribution remain to be verified |
| T21 File and recent models | Pass for tested cases | Tests cover persistence, equivalent-path deduplication, the ten-file limit, multiple app instances, failed opens, cancellation, and menu Esc handling. Native restart and recent-file selection passed |
| T22 Help dialogs | Pass for tested cases | Native review and egui tests checked actual build metadata, explicit unavailable revision, offline license notices, and Esc handling |
| T23 Dark theme | Not run in full | Native review confirmed dark menus, dialogs, panels, and Simulink and Stateflow canvases at 175% Windows scaling. The full display-scaling matrix remains pending |
| Display scaling and performance | Not run | Display scaling matrix and reference-machine measurements remain pending |

No failures remained in the final checks above. See [the design](slx_viewer_design.md#111-implementation-milestones) for the remaining milestones.

### 1.5.1 SLX used

The accepted saved SLX references are only `dc_motor_speed_control.slx` and models under `tests/fixtures/microgrid/` (the micro_grid collection). See the [fixture manifest](tests/fixtures/README.md) for hashes and verification status. Other local models are excluded from reference tests and compatibility evidence. Synthetic archives remain permitted for isolated tests. Microgrid parser and native acceptance checks have not run. Earlier evidence from removed fixtures does not establish current acceptance.
