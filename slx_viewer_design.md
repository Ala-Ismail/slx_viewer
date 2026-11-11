# 1. SLX Viewer Design

Status: the Windows desktop app opens and displays models and supports basic subsystem browsing. See [implementation and verification](README.md).

## 1.1 Product decision

Build a lightweight, standalone **Rust desktop application** for viewing Simulink `.slx` models. The application is strictly read-only: **no model editing and no simulation**. End users should not need MATLAB, Simulink, Python, or a separately installed Rust toolchain.

Start with Windows x64. Keep parsing and rendering portable so Linux and macOS builds can follow. Windows-first is a working assumption, not a restriction on the architecture.

The goal is to inspect saved diagrams, hierarchy, connections, and stored parameters. It is not to reproduce every behavior or visual detail of the Simulink editor.

## 1.2 Why Rust

Rust is the selected implementation language because deployment footprint, predictable resource use, and a self-contained desktop experience are priorities. This is an engineering judgment; actual installer size, startup time, and memory use must be measured on a release build.

| Consideration | Rust | Python with PySide6 |
| --- | --- | --- |
| Initial development | More upfront work, especially for diagram interactions | Usually faster for exploring XML and building a GUI |
| Distribution | Native executable with required platform libraries and assets | Bundled application includes Python and Qt dependencies |
| Parsing and memory control | Strong control over allocations and background processing | Convenient parsing, with runtime and object overhead |
| Diagram UI | Custom canvas work with egui | Mature Qt Graphics View scene and interaction framework |
| Fit for this project | **Selected for the lightweight standalone viewer** | Reasonable alternative when prototype speed is the main priority |

Qt offers a dedicated interactive 2D scene framework, and PySide6 has deployment tooling, so Python is a viable alternative. Its distribution and development tradeoffs simply differ from the chosen approach. See [Qt Graphics View](https://doc.qt.io/qt-6/graphicsview.html) and [PySide6 deployment](https://doc.qt.io/qtforpython-6/deployment/deployment-pyside6-deploy.html).

## 1.3 Scope

### 1.3.1 First release

- Open a local `.slx` file through a file dialog, drag-and-drop, or a command-line file argument.
- Provide File and Help dropdown menus, recent-file access, version and build information, and a license view.
- Use the dark color scheme defined in section 1.7.6.
- Display blocks, labels, ports, signal lines, branches, and basic text annotations using saved layout information.
- Pan, zoom around the pointer, fit the current diagram, and select objects for inspection.
- Browse the subsystem tree; double-click a subsystem to enter it; use breadcrumbs and back navigation.
- Inspect stored block type, name, identifier, parameters, and reference information in a read-only panel.
- Search block names and paths throughout the loaded model and navigate to a result.
- Show clear diagnostics for unsupported content, unresolved references, and malformed files.
- Work offline without a MATLAB or Simulink installation.

### 1.3.2 Explicit exclusions

- Creating, moving, resizing, connecting, deleting, or changing blocks or model properties.
- Saving or rewriting `.slx` files, autosaving models, or maintaining model undo/redo history.
- Simulation, compilation, code generation, solver configuration, or signal-value execution.
- Evaluating MATLAB expressions, model callbacks, initialization code, mask scripts, or embedded code.
- Computing compiled data types, dimensions, sample times, or active variants.
- Full Stateflow rendering, Simscape semantics, and legacy `.mdl` support in the first release.

Stored parameter values are displayed as text. For example, a gain saved as `Kp` remains `Kp`; the viewer does not attempt to resolve its workspace value.

## 1.4 Technology stack

| Area | Choice | Purpose |
| --- | --- | --- |
| Language/build | Rust stable and Cargo | Native application and repeatable builds |
| Desktop UI | `egui` with `eframe` | Window, panels, controls, and custom diagram canvas |
| Archive access | `zip` crate | Read SLX package entries without extracting the archive |
| XML parsing | `quick-xml` | Stream XML into a viewer-specific model |
| File dialogs | `rfd` | Platform file selection |
| Diagnostics | `thiserror`, `tracing` | Typed load errors and local diagnostic logging |
| Preferences | `serde` with a small local settings file | Window size, theme, and recent files only |

`egui` supports native applications through its integration framework; its custom painting APIs suit a diagram canvas. It does not supply Simulink rendering or a complete graph scene automatically. See the [egui project documentation](https://github.com/emilk/egui).

`quick-xml` provides a streaming reader suitable for processing individual package parts. See the [quick-xml documentation](https://docs.rs/quick-xml/latest/quick_xml/).

Use a worker thread and channels for loading. An asynchronous runtime is unnecessary for the initial local-file workflow. Pin compatible dependency versions and commit `Cargo.lock` when implementation starts. Select and test an eframe graphics backend on the intended Windows machines before finalizing distribution.

## 1.5 SLX compatibility strategy

MathWorks describes SLX as a compressed Open Packaging Conventions package containing UTF-8 XML and other data formats. Reading the archive is only the first step; a useful viewer must interpret model structure and saved geometry. See [MathWorks: Save Models](https://www.mathworks.com/help/simulink/ug/save-models.html).

Treat the internal representation as a compatibility boundary. Do not assume that every model consists of one XML file, that all releases use identical part names, or that all visual information is available without executing Simulink.

The loader should:

1. Validate the archive and index its entries.
2. Discover model parts through package metadata, relationships, and explicitly supported layout conventions.
3. Handle both embedded systems and separately referenced system parts when demonstrated by fixtures.
4. Parse systems, blocks, ports, connections, annotations, and stored properties.
5. Resolve references into an internal graph and retain diagnostics for unresolved objects.
6. Normalize saved geometry into a coordinate system independent of the UI.

Before claiming support for any Simulink release, validate files saved by that release. Maintain a compatibility matrix recording source release, features present, expected rendering, and known gaps. No release-wide compatibility claim is made by this design.

| Content | Viewer behavior |
| --- | --- |
| Basic blocks and local subsystems | Render saved layout and support navigation |
| Unknown block types | Draw a labeled generic block and expose stored properties |
| Masked blocks | Show saved metadata and a generic symbol when needed; never execute mask code |
| Library-linked blocks | Show available saved content and link information; mark missing definitions |
| Referenced models | Show the reference; first release does not automatically load another file |
| Variants | Show saved structure and stored conditions without evaluating which variant is active |
| Stateflow charts | Open supported saved charts from the owning block. See sections 1.6.2 and 1.7.4 |
| Specialized blocks | Show a placeholder with an explicit unsupported-detail indication |
| Missing or ambiguous geometry | Use a documented approximation and display a diagnostic |

Preserve original routing and placement where available. Avoid automatic graph layout for normally supported content because it would change the visual meaning of a saved diagram. Any necessary fallback routing must be identifiable as approximate.

### 1.5.1 Compatibility work

Support embedded and referenced systems, older core-properties release metadata, explicit and singleton control-port endpoints, stored port counts and labels, rich annotations, rotation, mirroring, and branches. Preserve unknown content with diagnostics.

Stateflow may be stored in a monolithic machine part or in referenced chart parts. A future referenced-chart adapter must resolve relationships relative to the owning part, preserve chart-local identifiers, enforce shared limits and cancellation, and diagnose missing or unsupported chart content. Release compatibility requires independently recorded saved-fixture evidence; no removed private-fixture evidence establishes current support.

## 1.6 Architecture

```text
Local SLX file (opened read-only)
              |
      Archive reader / part index
              |
      XML compatibility adapters
              |
      Normalized model + diagnostics
              |
      Per-system diagram scene
              |
      egui canvas + inspection panels
```

Separate parsing from rendering. The parser must work without opening a window, and the UI must never depend directly on XML nodes.

For multiple open models, keep an ordered collection of document tabs and an active tab ID. Each document owns its immutable model, scenes, navigation history, selection, hierarchy expansion, diagnostics, and per-system and per-chart viewports. Keep menus, recent files, theme, and build information at application level. See section 1.7.7 for the planned tab behavior.

Suggested module layout:

```text
src/
  main.rs
  app.rs                 # Application state and worker messages
  archive.rs             # Bounded package access and part resolution
  parser/                # XML readers and compatibility adapters
  model.rs               # Normalized, UI-independent representation
  geometry.rs            # Transforms, bounds, paths, port placement
  scene.rs               # Render primitives and hit testing
  ui/                    # Canvas, hierarchy, inspector, search, diagnostics
  settings.rs            # Local application preferences
tests/
  fixtures/              # Small, redistributable example models
  parsing.rs
  geometry.rs
```

### 1.6.1 Internal representation

- `Model`: metadata, root system, system index, diagnostics, and search index.
- `System`: stable internal ID, parent, blocks, connections, annotations, and source-part reference.
- `Block`: source identifier, name, type, bounds, orientation, ports, raw properties, and optional child-system reference.
- `Port`: owning block, kind, index, label, and stored or inferred anchor.
- `Connection`: optional source endpoint, branch tree, destination endpoints, saved path segments, and labels.
- `Annotation`: bounds, plain display text, and supported style properties.
- `Diagnostic`: severity, explanation, source part, and associated object where available.

Scope source identifiers by their owning model/system as required by observed files; do not assume block names or bare identifiers are globally unique. Keep unresolved endpoints explicit rather than inventing connections. Represent branches as a tree or equivalent shared-segment graph so fan-out is preserved.

The loaded model is immutable. Selection, navigation history, expanded tree items, and viewport transforms belong to separate UI state.

### 1.6.2 Stateflow parser


#### 1.6.2.1 Part discovery and block mapping

1. Follow the block diagram's internal Stateflow relationship and validate its target with the archive path rules before parsing the machine part.
2. Index machines, charts, and instances. Resolve each instance's machine and chart IDs, then match its saved block path against the loaded Simulink hierarchy. Compare path components and handle observed escaped line breaks consistently. Preserve original names and diagnose ambiguous matches.
3. Attach the chart to the owning Simulink block marked `SFBlockType=Chart`. The `sf_sfun` block inside the saved wrapper remains a regular Simulink block. Keep the wrapper's child system available in the normalized model.
4. Treat a model without Stateflow metadata as a normal Simulink model. Report missing chart data, ambiguous mappings, and unsupported chart types against the affected block or part. Retain usable Simulink diagrams when optional chart data is unavailable or malformed. Propagate cancellation and package-wide limit or path violations through the existing loader.

#### 1.6.2.2 Chart data

- Store chart identity, kind, owning block reference, and source part. Keep state, graphical function, and junction identities scoped to their chart and preserve their saved containment hierarchy.
- Store saved object bounds, labels, and transition geometry. Preserve transition labels, endpoint intersections, control points where present, and execution order as stored. Keep unresolved endpoints explicit and distinguish them from the absent source of a default transition.
- Keep state actions, transition conditions, and graphical function labels as inert text. A chart marked `EML_CHART` represents a MATLAB Function in the observed fixture and is outside the initial state-diagram parser scope.

## 1.7 Rendering and interaction

Build a scene for the current subsystem, retaining world coordinates separately from screen coordinates. Apply the same transform to drawing and hit testing.

Draw background annotations, connection paths, blocks and ports, labels, then selection overlays. Use simple hand-drawn symbols for a small set of common block types, such as Inport, Outport, Constant, Gain, Sum, and Subsystem. Use generic shapes for everything else.

Support saved rotations and mirroring in both block geometry and port placement. Normalize relative path segments where applicable, preserving branch junctions and endpoint references. Handle Unicode labels and font fallback deliberately.

Cache parsed model data, scene geometry, and expensive text layout. Cull objects outside the viewport, reduce label detail at distant zoom levels, and repaint on interaction or background-load completion. Avoid continuous idle redraws. Add spatial indexing only when profiling shows hit testing or culling needs it.

Suggested layout:

```text
+---------------------------------------------------------------+
| File | Help                                                   |
+---------------------------------------------------------------+
+---------------------------------------------------------------+
| ← | ↑ | Breadcrumbs                                    Search |
+----------------+----------------------------+-----------------+
| Model hierarchy|                            | Read-only       |
|                |       Diagram canvas       | properties      |
|                |                            |                 |
+----------------+----------------------------+-----------------+
| Read-only | Status / diagnostics          Fit | − | 100% | +   |
+---------------------------------------------------------------+
```

Display a persistent read-only indication. Selection highlights an object. There are no resize handles, connection handles, editable property controls, or save-model commands.

Use icon buttons **←** for Back and **↑** for Up in the navigation toolbar. Provide Back and Up tooltips, keyboard-shortcut hints, and accessible names. Disable the icons when their respective navigation actions are unavailable.

Place **Fit**, **−**, the current zoom percentage, and **+** together at the far right of the bottom status bar. Clicking the percentage resets zoom to 100%. Keep this group anchored to the window's bottom-right corner during resizing. Keep the read-only indication and status or diagnostic information on the left, allowing long status text to wrap or truncate before it crowds the controls. Apply these controls to the currently displayed Simulink system or Stateflow chart and disable them before a model is loaded.

### 1.7.1 Mouse and zoom controls

| Input over the diagram canvas | Action |
| --- | --- |
| Ctrl + mouse wheel up | Zoom in around the pointer |
| Ctrl + mouse wheel down | Zoom out around the pointer |
| Hold middle mouse button and drag | Pan the diagram |
| Left-button drag on empty background | Pan the diagram |
| Mouse wheel without Ctrl | Scroll vertically |
| Shift + mouse wheel | Scroll horizontally |
| Left click an object | Select it for inspection |
| Double-click a subsystem | Enter the subsystem |
| Fit button at the bottom right | Fit the current diagram into the canvas |
| Bottom-right −, percentage, and + controls | Zoom out, reset to 100%, or zoom in around the canvas center |

To zoom, hold **Ctrl** and rotate the mouse wheel; pressing the wheel's middle button is not required. Keep the model point under the pointer stationary during zoom. Use smooth multiplicative scaling, initially clamped to 10%-800%. Ctrl + wheel takes precedence over plain or Shift + wheel scrolling. Handle these gestures only when the pointer is over the canvas so the hierarchy and properties panels retain normal scrolling.

### 1.7.2 Navigating inside blocks and back up

Only blocks with an available child system can be entered. Ordinary blocks open their stored properties when selected; they do not have an interior diagram.

| Input or control | Action |
| --- | --- |
| Double-click a subsystem block | Open its child diagram |
| Enter with a subsystem selected and the canvas focused | Open its child diagram |
| Esc, Alt + Up, or the ↑ toolbar button | Go to the current system's parent |
| Click a breadcrumb ancestor | Jump directly to that ancestor system |
| Alt + Left or the ← toolbar button | Return to the previously viewed system |
| Click a system in the hierarchy tree | Open that system directly |

For example, from `Model > Controller > Filter`, **Esc** or **Alt + Up** opens `Controller`; clicking the `Model` breadcrumb jumps to the root. Up follows the model hierarchy, while Back follows navigation history, including jumps made through search or the tree. Disable Up at the root and Back when history is empty. Esc at the root does not navigate or close the application.

Esc first dismisses an open dialog, menu, or popup, or cancels an active text interaction. Consume that key event so it cannot also navigate. Otherwise, Esc goes up one subsystem level.

Remember pan, zoom, and selection separately for each visited system. When going up, select the subsystem block just exited and bring it into view if necessary. Keep the hierarchy tree and breadcrumbs synchronized with the displayed system.

Enable entry only when the child diagram is available in the loaded model, including saved contents of masked subsystems where supported. For unresolved references or unsupported interiors, keep the current view and show an explanatory diagnostic. Enter shortcuts must not activate while typing in search or another text field.

### 1.7.3 Left model hierarchy panel

Show a resizable, scrollable **Model Hierarchy** panel on the left of the diagram. Display the loaded model as the root and its available subsystems as nested, expandable tree nodes, including subsystems at any depth. Ordinary blocks remain visible in the diagram and search results; they are not tree nodes. Show an empty-state message before a model is opened.

Clicking a system name opens its diagram; the expand/collapse control only changes the tree. Highlight the system currently shown on the canvas, expand its ancestors as needed, and scroll the selected node into view when navigation occurs through a block, Esc, breadcrumbs, Back, or search. Preserve expanded nodes when navigating within a model. Opening a different model replaces the tree and resets its expansion state. Mark unavailable or unresolved subsystem contents and show a diagnostic instead of navigating to an unavailable diagram.

### 1.7.4 Stateflow chart view

Open a linked chart by double-clicking its block, pressing Enter with the selected block and canvas focused, or selecting its chart entry in the hierarchy. Include the owning Simulink system and chart name in the breadcrumbs. Up or Esc returns to the owner and selects the chart block. Back follows viewing history. Keep separate viewport and selection state for each chart instance.

Render saved states, nested states, graphical functions, junctions, default transitions, labels, and transition arrows on the same pan-and-zoom canvas. Approximate transition curves through the saved endpoint positions and midpoint and disclose the approximation in the chart view. Skip unresolved transitions and objects with unusable geometry. Separate Stateflow subviews are outside the initial chart view. Show saved text and properties in the read-only inspector.

### 1.7.5 File and Help menus

The File and Help menus provide the following actions.

Place a menu bar at the top of the window with **File** and **Help** dropdown menus. Place the **←** and **↑** navigation icons and breadcrumbs in the toolbar below it. Fit and zoom controls belong at the bottom-right of the status bar. The empty-window Open action remains available before a model is loaded.

#### 1.7.5.1 File

| Menu item | Behavior |
| --- | --- |
| Open… | Open the native SLX file dialog. Ctrl+O invokes the same action |
| Open Recent | Expand a submenu listing up to ten successfully opened local models, newest first |
| Close Tab | Close the active document or loading tab. Ctrl+W invokes the same action; disable when no tabs exist |

Each recent-file entry shows the filename and enough of its path to distinguish models with identical names. Expose the full path on hover. Disable Open Recent when the list is empty.

Persist recent paths in the user's application settings directory. Update the list after a successful load from any entry point, deduplicate equivalent paths, and move the most recently opened model to the top. An unavailable recent file produces an actionable message and leaves the current model usable. Recent-file selection uses the existing background loader, cancellation, and stale-result protection.

#### 1.7.5.2 Help

| Menu item | Behavior |
| --- | --- |
| Current Version and Build Information… | Open a read-only dialog with application version and build details |
| License… | Open a read-only view of the application license and bundled third-party notices |

The version dialog shows the application name, Cargo package version, build timestamp in UTC, source revision when available, target platform and architecture, build profile, and active graphics backend. Obtain these details from the build and runtime metadata. Display unavailable fields explicitly when the source checkout or build environment cannot supply them.

The license view displays the project's chosen license text and the notices required by bundled dependencies and assets. Bundle this information for offline access. The project uses the MIT License in LICENSE; the dialog embeds that text. Dependencies and fixtures retain their respective notices and license terms.

Support standard menu keyboard interaction, including Alt+F and Alt+H, arrow-key navigation, and Enter to activate a selection. Esc closes the active submenu, menu, or Help dialog and consumes the key event before parent-diagram navigation. Keep model and viewport state when dismissing these controls.

### 1.7.6 Dark color scheme

Use a dark theme by default for the entire application, including the menu bar, dropdowns, dialogs, hierarchy, inspector, diagnostics, Simulink canvas, and Stateflow canvas. Persist the theme preference in application settings.

| Role | Color |
| --- | --- |
| Diagram canvas | `#11151B` |
| Panels, menu bar, and dialogs | `#1B212B` |
| Block and Stateflow state fill | `#242D39` |
| Hovered controls | `#303D4E` |
| Primary text | `#E7EDF5` |
| Secondary text | `#AAB8CA` |
| Connections, transitions, and arrowheads | `#CBD5E1` |
| Block and state outlines | `#8B9DB3` |
| Selected-object fill | `#173F5F` |
| Selection outlines and keyboard focus | `#66C2FF` |
| Graphical function outlines | `#A7B9FF` |
| Warning text | `#FFD180` |
| Error text | `#FF9B9B` |

Apply these roles consistently to generic blocks, nested states, junctions, transition labels, and selected objects. Use visible outlines and focus indicators alongside color. Verify text and geometry remain legible at supported zoom levels and Windows display scaling settings. The native file dialog follows Windows appearance.

### 1.7.7 Multiple SLX model tabs

Initial tab support is implemented: successful opens retain previous documents, switching restores document state, canonical paths reuse existing tabs, and close buttons and Ctrl+W close tabs. The full behavior below remains the target. Queued per-tab loading, keyboard cycling, File > Close Tab, path-disambiguated labels, and T28/T29 verification remain pending. Formatting, Clippy, 23 existing tests, and the locked release build passed; native tab review and new tab acceptance tests have not run.

Place a model tab strip between the File/Help menu bar and the navigation toolbar. File > Open, Open Recent, a dropped SLX file, and a launch path open a new tab and activate it. Opening a canonical path already present activates its existing tab instead of loading a duplicate. Allow different files with identical names; show a distinguishing parent path in their tab labels and the full path on hover. Use a horizontally scrollable strip so many tabs do not push the canvas downward. Include a visible close button and keyboard focus indication on each tab. Enclose the model name and close button within one shared tab border; neither control has a separate border. Style tabs as attached document tabs: use the display toolbar background for the active tab, retain a highlighted top and side outline, and omit its bottom border so it joins the display area. Inactive tabs use the darker strip background and a bottom divider. Keep one shared boundary around the name and close control.

Switching tabs restores that document's current system or Stateflow chart, Back history, selection, expanded hierarchy nodes, search state, pan, and zoom. Bind the hierarchy, breadcrumbs, properties, diagnostics, window title, status, and bottom-right Fit and zoom controls to the active tab. Keep File and Help menus available regardless of the active document. Ctrl+Tab and Ctrl+Shift+Tab cycle forward and backward through tabs, wrapping at the ends, unless a modal dialog or active control consumes the shortcut. Esc continues to navigate within the active model; it does not close a tab.

Closing a tab releases its model and scene caches without a save prompt because viewing is read-only. Closing the active tab activates the next tab to its right, or the previous tab if it was last. Closing the final tab restores the empty Open-model window. Closing an inactive tab preserves the active document and its viewport. Open tabs are session-only; retain the existing recent-file persistence without automatically reopening tabs on startup.

Give every loading tab a stable ID and request generation. Show its loading state and support cancellation without blocking interaction with other tabs. Route results only to the matching live tab and generation; completion must not steal focus from another active tab. Closing or cancelling a loading tab invalidates its outstanding result. A failed load shows an actionable error in its own tab and leaves other documents usable. Update recent files only on successful completion. Deduplicate in-flight requests for the same canonical path as well as loaded documents.

Retain per-model archive and XML limits and bound concurrent parsing to one worker initially, queueing additional requests with per-tab cancellation. Do not preload scenes for inactive diagrams. Measure aggregate memory with several open models before selecting an overall document or memory limit; report allocation failures without discarding existing documents. Multi-model performance claims require separate evidence from single-model measurements.

## 1.8 File handling and failure behavior

Open input files with read access only. Do not extract archive contents to arbitrary filesystem paths. Validate internal part references and enforce configurable limits on entry count, expanded bytes, XML nesting, and object count. Reject external XML entities and never execute or fetch embedded content.

Load in the background with cancellation. In the planned tabbed workflow, tag each request with its document tab ID and generation so a stale result cannot populate a closed or superseded tab. A failed load leaves other open models usable and presents an actionable message in its own tab.

Distinguish fatal failures, such as an invalid archive or unreadable root structure, from recoverable gaps, such as an unknown block icon. Recoverable gaps should retain the rest of the diagram and attach diagnostics to affected objects.

Application preferences may be written to the user's settings directory. The source model is never modified.

## 1.9 Performance targets

These are provisional acceptance targets, not measured results. Establish a named Windows reference machine and fixture corpus before judging them.

| Measure | Initial target |
| --- | --- |
| Cold launch to empty window | Under 2 seconds |
| Load a representative 10,000-block model | Under 3 seconds |
| Pan/zoom a system with 2,000 visible blocks and representative wiring | Approximately 60 FPS on the reference machine |
| Peak process memory for the 10,000-block fixture | Under 250 MB |
| Compressed Windows distribution | Aim for under 50 MB |
| Idle activity | No continuous repaint loop; near-idle CPU |

Record archive size, expanded data size, connection count, label density, and hierarchy depth with measurements. Block count alone is not enough to characterize performance. Measure packaged release builds; do not infer results from language choice.

## 1.10 Validation

Use small, redistributable SLX fixtures covering common blocks, nested systems, branches, rotated/mirrored blocks, Unicode labels, annotations, and unresolved references. Obtain reference screenshots from the same saved fixtures where Simulink is available during development; end users do not need Simulink.

Check parser output for object counts, identities, hierarchy, properties, and connectivity. Test geometry transforms, branch normalization, and endpoint placement independently from the GUI. Compare rendered fixtures against expected diagrams with tolerance for fonts and platform differences.

Exercise corrupt archives, missing parts, excessive expansion, deeply nested XML, cancellation, and switching files during loading. Verify the input file hash is unchanged after opening, navigating, searching, and closing. Smoke-test the packaged executable on a clean Windows machine without development runtimes.

### 1.10.1 Fixtures and evidence

The accepted saved SLX reference corpus is limited to `tests/fixtures/dc_motor_speed_control.slx` and SLX files under `tests/fixtures/microgrid/` (the micro_grid collection). Other local models must not be used in reference tests or compatibility claims. Synthetic archives remain permitted for parser and failure cases. The current microgrid model is `pvwindupfc11.slx`, declaring R2016a; its parser and native acceptance checks have not run. Stateflow saved-fixture and native evidence must be established using the accepted corpus where those features exist.

Keep a fixture manifest recording filename, SHA-256, provenance and redistribution permission, source Simulink release where known, covered features, and expected results. Include a three-level hierarchy (`Model > Controller > Filter`), repeated block names across systems, branches, rotations/mirroring, Unicode, annotations, unsupported blocks, and unresolved references.



Record each criterion as Pass, Fail, or Not run, with build identifier, fixture, platform, and evidence. The [milestone verification report](README.md#15-verification) records completed checks and outstanding release criteria.

### 1.10.2 Functional acceptance criteria

| ID | Scenario | Pass condition | Verification |
| --- | --- | --- | --- |
| T01 | Open a supported SLX | Dialog, drag-and-drop, and command-line routes show the same root diagram; paths with spaces and Unicode work | Parser integration and desktop smoke tests |
| T02 | Parse model contents | Systems, identifiers, properties, ports, and connection endpoints match independently recorded fixture expectations | Automated integration tests |
| T03 | Render saved geometry | Branch destinations and junctions are correct; rotated/mirrored ports remain attached; supported labels and annotations appear at saved positions | Geometry tests and reference-image review |
| T04 | Ctrl + wheel zoom | Up zooms in and down zooms out; scale stays within 10%-800%; the pointer anchor moves by at most one logical screen pixel | Transform tests and desktop input test |
| T05 | Pan and scroll | Middle drag and left drag on empty background pan without moving blocks; plain wheel scrolls vertically, Shift + wheel horizontally; scrolling panels does not zoom the canvas | Interaction tests and desktop input test |
| T06 | Fit and zoom reset | Fit includes all supported diagram objects and label bounds with padding; reset gives 100%; empty diagrams produce no invalid transform | Geometry tests and desktop smoke test |
| T07 | Enter subsystem | Double-click or Enter with canvas focus opens an available child system; ordinary blocks do not navigate | Navigation tests and desktop input test |
| T08 | Esc to parent | Filter goes to Controller, then Model, one level per press; the exited subsystem is selected; Esc at root neither navigates nor quits | Navigation tests and desktop input test |
| T09 | Esc event priority | A dialog/menu/popup or text interaction consumes Esc without also navigating; a subsequent unconsumed Esc goes to the parent | Event-handling tests and desktop input test |
| T10 | Other navigation | Tree and breadcrumbs stay synchronized; Up follows parentage, Back follows history; revisiting restores the viewport except adjustments needed to reveal the exited subsystem | Automated navigation tests |
| T11 | Search and inspect | Results find expected names and paths, distinguish duplicate names, and navigate to the correct object; stored expressions such as `Kp` remain literal text | Search tests and UI review |
| T12 | Unsupported content | Placeholders or diagnostics identify gaps while supported content remains usable; unavailable interiors do not navigate | Integration tests and visual review |
| T13 | Read-only behavior | Input SHA-256 is unchanged after all viewing workflows; a filesystem-read-only model opens; no editing or save-model commands exist | Integration tests and UI review |
| T14 | Embedded code | Callbacks, mask code, and expressions remain inert; external entities are rejected; loading neither executes commands nor fetches external resources | Adversarial fixtures, process/network observation, and code review |
| T15 | Invalid or excessive input | Invalid ZIP/XML, missing root parts, traversal references, and resource-limit violations fail with actionable errors, without crashing or unbounded allocation | Negative tests at and beyond configured limits |
| T16 | Background loading | Cancellation leaves the UI usable; failure preserves the previous model; an older request cannot replace a newer one | Worker tests with controlled completion order |
| T17 | Standalone distribution | Packaged release launches and opens supported fixtures offline on clean Windows without MATLAB, Python, Rust, or developer tools | Packaged desktop smoke test |
| T18 | Left hierarchy panel | Root and nested systems appear in a resizable, scrollable tree; expand/collapse does not navigate; selecting a node opens it; block navigation, Esc, breadcrumbs, Back, and search highlight the current system and reveal its ancestors; opening another model replaces the tree | Navigation tests and desktop smoke test |
| T19 | Parse saved Stateflow charts | Saved hierarchy, labels, geometry, and endpoints match independently recorded expectations; invalid references produce diagnostics | Synthetic parser cases retained; saved-fixture verification not run |
| T20 | Open and inspect Stateflow charts | Hierarchy, double-click, and Enter open charts; Esc selects the owner; Back and reopening preserve viewport and selection | Saved-fixture UI and native verification not run |
| T21 | File menu and recent models | File > Open and Ctrl+O open the same dialog. Open Recent persists up to ten unique successful paths in newest-first order, distinguishes repeated filenames, and handles missing files while retaining the current model. Esc closes a menu without navigating the diagram | Settings tests, egui input tests, and native menu smoke tests |
| T22 | Version, build, and license views | Help dialogs show actual version and available build metadata, clearly identify unavailable fields, and display the project license status and bundled notices offline. Esc dismisses the active dialog without navigating the diagram | Build-metadata checks and desktop review |
| T23 | Dark color scheme | Menus, dialogs, panels, and both diagram canvases use the specified dark roles. Text, transitions, selection, and keyboard focus remain legible at supported display scaling settings | Theme and settings checks and native visual review |
| T27 | Navigation icons and bottom-right zoom controls | Back and Up use ← and ↑ icons with tooltips and accessible names. Fit, −, the clickable zoom percentage, and + remain anchored to the status bar's right edge while resizing. Controls operate on the active system or chart and stay visible alongside long status messages | UI input tests and native Windows layout review; passed for tested sizes, see the README verification report |


Planned tab acceptance checks supplement the table above:

| ID | Scenario | Pass condition | Verification |
| --- | --- | --- | --- |
| T29 | Tab lifecycle and loading | Close buttons and Ctrl+W follow the specified selection rules; closing the final tab restores the empty window. Keyboard cycling works. Queued, cancelled, failed, closed, and out-of-order loads cannot replace another document or steal focus. Overflow tabs remain reachable and input hashes remain unchanged | Planned, not run. Controlled worker tests, UI input tests, native overflow and read-only review |

### 1.10.3 Performance and release acceptance

Measure section 1.9 targets on a recorded reference machine using a release build. Record OS, CPU, RAM, GPU, graphics backend, display scaling, and fixture hashes. Report median and worst durations from five launches and five loads, distinguishing cold and warm runs. Record frame times during a repeatable 30-second pan/zoom sequence, peak memory during loading/navigation, packaged size, and idle CPU/repaint count over 30 seconds after loading settles.

Performance targets remain provisional until the reference machine and corpus are established. Report deviations explicitly; do not claim an unmeasured target passed. Document the reason for any target revision.

Once the Cargo project exists, run:

```text
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo build --release --locked
```

All applicable functional criteria must pass before the first release, with evidence for manual checks. Not run is not Pass. Record compatibility exclusions, repeat affected checks after changes, and run the full release checklist on the packaged candidate.

## 1.11 Implementation milestones

1. **Format feasibility:** collect fixtures, inspect package layouts, and load a basic model into the normalized representation. Document what is confirmed and what remains uncertain.
2. **First view:** open a file and render basic blocks, labels, ports, and connections with pan, zoom, and fit. Validate the chosen graphics backend.
3. **Model inspection:** add subsystem navigation, hierarchy tree, selection, stored-property inspection, and search. Add File and Help menus, recent-file settings, and the default dark color scheme.
4. **Compatibility:** handle branches, orientation, annotations, unknown blocks, references, and visible diagnostics against the fixture matrix. Add the headless Stateflow parser, chart-to-block mapping, and the initial chart view. Complete the observed-layout changes and paired-release verification in section 1.5.
5. **Distribution:** measure performance, add bounded parsing and cancellation checks, and produce a tested Windows package with dependency notices. Supply version and build metadata and the offline license view.

Planned extension: implement multiple SLX document tabs using section 1.7.7, then verify T28 and T29 and measure aggregate memory before claiming multi-model capacity.

The first usable milestone is a standalone executable that opens a basic SLX file and displays its saved diagram. The first release is complete when the supported fixture matrix passes, unsupported content is clearly reported, and all workflows remain strictly read-only.

### 1.11.1 Sum and Product input operators

Display saved Inputs operators inside Sum and Product blocks next to their input anchors. Numeric input counts expand to addition or multiplication operators. Explicit signs show addition, subtraction, multiplication, and division per input; spacer characters do not consume a port. Follow the existing estimated port geometry, including rotation and mirroring. Missing Inputs retains the generic block symbol rather than guessing an operation. Native geometry review remains pending.

### 1.11.2 Control signal connections

Resolve singleton enable, trigger, and ifaction destinations to control port index 1, retaining explicit indexed endpoints. Render their saved routes with existing estimated control-port anchors, transformed for block rotation and mirroring. The enable endpoint regression test passes; native geometry review remains pending.
