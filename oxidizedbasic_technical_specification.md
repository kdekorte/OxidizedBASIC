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

Classic Visual BASIC controls are mapped directly to high-performance GPU-rendered **GPUI Kit** components:

| Classic BASIC Control | GPUI Kit / GPUI Equivalent | Description & Behavior |
| :--- | :--- | :--- |
| `Form` | `gpui::Window` / `Root` | Top-level GPU window with Flex layout engine |
| `CommandButton` | `gpui_kit::component::button::Button` | Interactive button with hover/active/focus states |
| `TextBox` | `gpui_kit::component::input::Input` / Rope Editor | High-performance text edit field |
| `Label` | `gpui::elements::Label` / `Text` | GPU text-shapen static text display |
| `Frame` | `gpui::div().border()` | Flexbox container with customizable border/padding |
| `ListBox` | `gpui_kit::component::table` / Virtualized List | Virtual-scrolled list item view |
| `ComboBox` | `gpui_kit::component::select` | Dropdown select component |
| `PictureBox` / `Image` | `gpui::img()` | Accelerated image loader & canvas frame |
| `Timer` | `gpui::Context::spawn` / Async Timer | Non-blocking async loop timer thread |

### 5.1 Event Dispatcher Engine
The GPUI event model communicates with the BASIC runtime through Rust channel message passing (`async_trait` and GPUI `Context` emitters):

```rust
// Standard CommandButton Click Bridge Example
button.on_click(cx.listener(|this, _event, window_cx| {
    this.runtime.evaluate_sub("Command1_Click", vec![]);
}));
```

---

## 6. IDE Architecture (`oxbasic_ide`)

The OxidizedBASIC IDE is a native GPUI application providing sub-millisecond editor responsiveness:

1. **Form Canvas (Visual Designer):** Drag-and-drop form layout canvas using absolute positioning or flexbox bounds. Visual handles allow resizing controls, outputting `.oxform` JSON layout specifications.
2. **Code Editor:** Utilizes Tree-sitter for instant syntax highlighting, diagnostics, and code folding backed by a fast Rope data structure.
3. **Property Inspector:** Reactive side panel rendering active widget properties (Colors, Dimensions, Captions, Events).
4. **Immediate Window / REPL:** Interactive execution window connected directly to the `oxbasic_runtime` environment context.

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
- Build the visual Form Designer in GPUI Kit.
- Integrate Tree-sitter syntax highlighting editor and Property Inspector.
- Implement debug controls (Breakpoints, Step-over, Variable Watch).

### Phase 4: JIT Codegen & Packaging (Months 10–12)
- Integrate Cranelift JIT backend for compiling BASIC routines directly to native assembly.
- Package bundle generator producing self-contained native binaries for macOS (`.app`).