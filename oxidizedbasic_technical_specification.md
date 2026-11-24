# Technical Specification: OxidizedBASIC (`oxbasic`) Engine & IDE

## 1. Executive Summary & Objective

### 1.1 Purpose
This specification outlines the technical architecture, implementation roadmap, and system design for **OxidizedBASIC** (`oxbasic`). 

OxidizedBASIC is an open-source, ultra-fast, memory-safe classic BASIC programming suite written entirely in **Rust**. It provides full syntax and semantic compatibility with Visual Basic 6.0 (VB6) and classic VBA, while using **GPUI** and **GPUI Kit** to deliver 120 FPS, GPU-accelerated application interfaces and a modern IDE on macOS, Windows, and Linux.

### 1.2 Key Objectives & Core Technology
- **Pure Rust Stack:** Zero dependency on legacy C++ COM/OLE subsystems or browser wrappers (Electron/Tauri). Built from the ground up using idiomatic Rust.
- **GPU-Accelerated Interfaces:** Both the IDE and compiled/interpreted BASIC user interfaces are rendered using **GPUI** (via GPU Metal/Vulkan backends) and **GPUI Kit** component primitives (`gpui-component`, `gpui-base`).
- **High-Performance Parser & JIT/Interpreter:** Built with `pest` or `tree-sitter-basic`, generating an intermediate representation (IR) targeting Cranelift JIT or LLVM (`inkwell`) for near-native execution speed.
- **Cross-Platform Readiness:** Designed for macOS native performance while maintaining direct portability to Linux and Windows via GPUI multi-platform rendering capabilities.

---

## 2. Architecture Overview

```
+-------------------------------------------------------------+
|               OxidizedBASIC IDE Layer (Rust)                |
|  [ Visual Canvas ]     [ GPUI-Kit Editor ]    [ Inspector ] |
+-------------------------------------------------------------+
                               |
                               v
+-------------------------------------------------------------+
|             Compiler Front-End (`oxbasic_parser`)           |
|   [ Lexer / Pest Parser ] -> [ AST ] -> [ Semantic Pass ]   |
+-------------------------------------------------------------+
                               |
                               v
+-------------------------------------------------------------+
|            Execution & Codegen (`oxbasic_runtime`)          |
|  [ IR Generator ] -> [ Cranelift JIT / Bytecode Interpreter ]|
+-------------------------------------------------------------+
                               |
                               v
+-------------------------------------------------------------+
|                GPUI / GPUI Kit Render Bridge                |
|   [ Element Mapping ] <-> [ Reactive Event Dispatcher ]     |
+-------------------------------------------------------------+
```

---

## 3. Rust Crate Workspace Structure

The project is structured as a cargo workspace:

```text
oxidized-basic/
├── Cargo.toml
├── crates/
│   ├── oxbasic_ast/         # AST data structures & symbol definitions
│   ├── oxbasic_parser/      # Grammar, lexer, and parser (Pest/Nom)
│   ├── oxbasic_runtime/     # Interpreter, Variant system, stdlib, JIT engine
│   ├── oxbasic_ui/          # GPUI & GPUI Kit widget mapping runtime engine
│   └── oxbasic_ide/         # The desktop visual IDE app binary
```

---

## 4. Language & Runtime Specification

### 4.1 Memory & Type System (`oxbasic_runtime`)
To replicate legacy BASIC dynamic typing within Rust’s strict ownership model, `Variant` is implemented as a tagged enum with shared interior mutability (`Arc<Mutex<T>>` or `Rc<RefCell<T>>`):

```rust
pub enum Variant {
    Empty,
    Null,
    Integer(i16),
    Long(i32),
    LongLong(i64),
    Single(f32),
    Double(f64),
    String(String),
    Boolean(bool),
    Date(chrono::NaiveDateTime),
    Object(Option<Arc<Mutex<dyn BasicObject>>>),
    Array(Vec<Variant>),
}
```

### 4.2 Syntax & Language Features
- **Control Flow:** `If...Then...Else`, `Select Case`, `For...Next`, `For Each...In`, `Do While/Until...Loop`.
- **Error Handling:** Native implementation of `On Error GoTo <label>` and `On Error Resume Next` using Rust's `Result` stack-unwinding mechanism.
- **Dynamic Invocation:** Early & Late binding runtime execution using Rust trait dispatch.

---

## 5. Visual Component Engine (GPUI & GPUI Kit Mapping)

Classic Visual BASIC controls are drawn directly by `oxbasic_ui`'s `paint_control`
(`crates/oxbasic_ui/src/gpui_controls.rs`) as GPUI `div()` elements positioned
from the form's coordinates. Each kind is one control in the form model
(`ControlKind`), and the same drawing serves the designer, the IDE's live form
windows and the packaged app.

| Classic BASIC Control | GPUI Equivalent | Description & Behavior |
| :--- | :--- | :--- |
| `Form` | `gpui::Window` | Top-level GPU window; its menu bar is the application's menu while it is in front |
| `CommandButton` | `div()` with hover/active states | Interactive button; `Click` runs its handler |
| `TextBox` | Text field with caret and selection | Editable text; `Change` on each edit |
| `Label` | Text `div()` | Static text, on the form background or its `BackColor` |
| `Frame` | Bordered `div()` with a caption | Container: its controls move and disable with it |
| `ListBox` / `ComboBox` | Virtual-scrolled list / drop-down list | Selection, `Click` / `Change`; `ComboBox` `Style` gives an editable or drop-down field |
| `CheckBox` / `OptionButton` | Box or circle with a label | `Value` 0 / 1 / 2 (CheckBox) or 0 / 1 (OptionButton) |
| `PictureBox` / `Image` | `gpui::img()` and a drawn canvas | `Picture`, `Stretch`, and `Line`/`Circle`/`PSet` drawn into the box |
| `HScrollBar` / `VScrollBar` | Track with a draggable thumb | `Value` within `Min`..`Max`; `Change` and `Scroll` |
| `Slider` | Track with a thumb | A draggable range, `Orientation` horizontal or vertical |
| `ProgressBar` | Track filled to `Value` | Set by the program; it takes no input |
| `TabControl` | A strip of tabs over a page | `Tab` is the selected page; the controls with `Container` = the tab's control and `Page` = its index show only on that page; a tab press or `Tab` raises `Click` |
| `Shape` / `Line` | Painted rectangle, oval or line | Drawn no further than the form's edge |
| `VolumeListBox` / `DirListBox` / `FileListBox` | List of volumes / folders / files | `Text`, `Path`, `Pattern`, `FileName`; `DblClick` on a file |
| `Timer` | Async timer task | Not drawn; `Timer` event at `Interval` |

### 5.1 Event Dispatcher Engine
Events travel from the window to the BASIC runtime through the debug session:
the window sends a `UserEdit` (a click, a value, a tab) to the program, and the
program's changes come back as a `FormSnapshot` (the form, its per-control state,
and which controls the program touched). `FormSnapshot::apply_to` applies the
program's copy of each touched control to the window, so keystrokes typed while
a snapshot is in flight are kept. Handlers are named `<Control>_<Event>` and run
in the program; `wire()` attaches the pointer, focus and key handlers to each part
of a control.

Menu items (`Menu` on a form) are GPUI actions. A menu item is enabled only while
its form's window holds the focus, because the action's handler sits on the
focus path; a form window therefore focuses its form when it opens and when it
becomes active.


---

## 6. IDE Architecture (`oxbasic_ide`)

The OxidizedBASIC IDE is a native GPUI application providing sub-millisecond editor responsiveness:

1. **Form Canvas (Visual Designer):** Drag-and-drop form layout canvas using absolute positioning or flexbox bounds. Visual handles allow resizing controls, outputting `.oxform` JSON layout specifications.
2. **Code Editor:** Utilizes Tree-sitter for syntax highlighting, syntax diagnostics (a red underline on the first line the parser cannot read), and code folding for blocks. The editor keeps its text as lines (`Vec<String>`), not a rope: a rope was benchmarked and was faster for edits, but the gain did not justify rewriting the editing core for typical module sizes.
3. **Property Inspector:** Reactive side panel rendering active widget properties (Colors, Dimensions, Captions, Events).
4. **Immediate Window / REPL:** Interactive execution window connected directly to the `oxbasic_runtime` environment context.
5. **Menu Editor:** Properties → Menu Editor… edits a form's `menus` list (New, Delete, Indent, Outdent, Apply, and shortcuts from modifier toggles and a key box). Closing the window keeps the menu.
6. **Form windows:** Each form the program shows opens its own window (`FormRunner` in the IDE, the same runner in a packaged app). The form's menu becomes the application's menu bar while its window is in front; the IDE's Edit, View and Run stay in the bar.

### 6.1 Packaged Apps
- `scripts/make_app.sh` builds the release `oxbasic` binary and packages it as `dist/OxidizedBASIC.app` (the IDE, started with `ide`), with the app icon and an ad-hoc signature.
- `oxbasic play <project.oxproj>` runs a project as an app with no editor or designer: it starts the program on the IDE's debug session and opens a window for each form the program shows. The app ends when the program ends or its last form window closes. Its own menu bar holds About and Quit.
- A notarised, Developer ID-signed copy for another Mac is not produced yet.


---

## 7. Implementation Roadmap

### Phase 1: Syntax Parser & Core Runtime (Months 1–3)
- Build AST and grammar definition using `pest` crate in `oxbasic_parser`.
- Implement runtime `Variant` system and basic execution engine in `oxbasic_runtime`.
- Implement standard BASIC string, math, and date function libraries.

### Phase 2: GPUI Element Binding Engine (Months 4–5)
- Create `oxbasic_ui` crate mapping BASIC layout coordinates to GPUI elements.
- Implement event listener registry connecting GPUI input/mouse events to BASIC subroutines.

### Phase 3: IDE Development (Months 6–9)
- Build the visual Form Designer in GPUI Kit. (Done: drag, resize handles, toolbox, Property Inspector.)
- Integrate Tree-sitter syntax highlighting editor and Property Inspector. (Done: the editor parses the whole module once and colours every row from that parse, so a block's `Sub` and `End If` lines get grammar colours. A row the grammar cannot colour falls back to a hand-written tokenizer. The drawing statements `Line`, `PSet` and `Circle` are in the grammar: `PSet` and `Circle` always, and `Line` only when a point follows it (`Line (x, y)` or `Line -Step(x, y)`), so `Line = 5` stays a variable.)
- Implement debug controls (Breakpoints, Step-over, Step-into (F8), Variable Watch). (Done.)
- Editor diagnostics are done: a line the parser cannot read is underlined, and its message shows in the status line with the caret on it (`parse_diagnostic` in the parser's `check`). Only the first error is marked. The editor stores lines in a `Vec<String>`, not a rope, by decision: a rope was benchmarked on a 20,000-line module and was about 5x faster for edits (2.1ms against 11.0ms for 4,000 edits), but a typical BASIC module is a few hundred lines where both are instant, and the editing core (caret, selection, undo) would have to be rewritten to gain it. Code folding is done for blocks (Sub, Function, loops, If, Select, With, Type, Enum). Unknown names are reported, inside expressions as well as on their own lines, and misspelled members are reported for every control kind the runtime handles (the member tables are checked against the runtime by a test, so they cannot drift from it).

### Phase 4: JIT Codegen & Packaging (Months 10–12)
- Integrate Cranelift JIT backend for compiling BASIC routines directly to native assembly.
- Packaging: `scripts/make_app.sh` produces the macOS `.app` (done for the IDE and for `oxbasic play`); a Developer ID-signed and notarised build for other Macs is still to do.
- Performance: revisit the interpreter's speed. The benchmark `examples/basica.bas` runs in about 1.26s against 0.34s for basika (a C interpreter). Most of the time is spent looking variables up by name (lowercasing and hashing on every access). Resolving names to slots at load time is the likely fix; the JIT work should be measured against this baseline.
