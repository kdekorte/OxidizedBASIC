# OxidizedBASIC (`oxbasic`)

An open-source, ultra-fast, memory-safe classic BASIC programming suite
written entirely in Rust, per `oxidizedbasic_technical_specification.md`.
Full syntax and semantic compatibility with Visual Basic 6.0 / classic VBA
is the goal; GPUI-based GUI rendering and the visual IDE follow in later
phases.

## Status — Phases 1 & 2 complete

### Phase 1: syntax parser & core runtime
- **Parser** (`oxbasic_parser`): `pest` grammar for the classic VB6/VBA
  statement surface — `If/Then/ElseIf/Else` (block + single-line),
  `Select Case` (values, `To` ranges, `Is` comparators, `Case Else`),
  `For…Next [Step]`, `For Each…In`, `Do While/Until…Loop` (pre- and
  post-test), `While…Wend`, `Sub`/`Function` with `ByVal`/`ByRef`/
  `Optional`/`ParamArray`, `Dim`/`Static`/`Const`, arrays, labels,
  `GoTo`, `On Error GoTo <label>|0`, `On Error Resume Next`, `Resume [Next|label]`,
  `Exit {Sub,Function,For,Do}`, `End`, `Option Explicit`, `Print`/`Debug.Print`
  with `;`/`,` separators, colon-separated statements, `&H`/`&O` literals,
  `'` and `Rem` comments, and full VB operator precedence.
- **Runtime** (`oxbasic_runtime`): tree-walking interpreter with a
  VB-style `Variant` tagged enum, `Rc<RefCell<…>>`-shared scopes so
  `ByRef` parameters mutate callers, multi-dimension fixed arrays plus
  auto-growing dynamic arrays, trappable `On Error` handling with the
  `Err` object, a ~70-function standard library, and `evaluate_sub` —
  the UI event bridge.

### Phase 2: GPUI element binding engine
- **Form model** (`oxbasic_ui`): render-agnostic classic VB forms and
  controls (`Form`, `CommandButton`, `Label`, `TextBox`, `Frame`,
  `ListBox`, `ComboBox`, `PictureBox`, `Timer`) with VB layout
  coordinates, `.oxform` JSON layout files, and the event dispatcher
  that routes UI events to BASIC subs (`Command1_Click`, `Form_Load`,
  `Timer1_Timer`, …).
- **GPUI backend** (`oxbasic_ui`, `gpui` feature): maps the form to a
  real GPU-accelerated GPUI window (Metal on macOS) — absolutely
  positioned controls with hover/active button states, focusable
  text boxes with keyboard input firing `Change`, clickable list box
  rows, and async background timers firing `Timer` events — all
  dispatched into the live BASIC interpreter.
- **CLI**: `oxbasic gui <form.oxform> <program.bas>` opens the window,
  runs module-level code, fires `Form_Load`, and serves events until
  the window closes.

## Usage

```sh
cargo build -p oxbasic_ide
./target/debug/oxbasic run examples/demo.bas                    # console program
./target/debug/oxbasic gui examples/clickme.oxform \
                      examples/clickme.bas                      # GPUI form app
./target/debug/oxbasic repl                                     # immediate window
cargo test --workspace                                          # 45 tests
```

`.oxform` layout files are JSON:

```json
{
  "name": "Form1", "caption": "Demo", "width": 340, "height": 240,
  "controls": [
    { "name": "Command1", "kind": "CommandButton",
      "rect": { "left": 16, "top": 48, "width": 100, "height": 30 },
      "caption": "Click Me" }
  ]
}
```

Controls map to GPUI elements per spec §5 (see `oxbasic_ui`'s mapping
table); events dispatch to `<ControlName>_<Event>` subs in the BASIC
program. `scripts/gui_smoke.sh` verifies the window opens, `Form_Load`
fires, and the async timer drives `Timer1_Timer`.

## Workspace layout

```
crates/
├── oxbasic_ast/         # AST data structures & symbol definitions
├── oxbasic_parser/      # Grammar (pest), lexer, parser
├── oxbasic_runtime/     # Interpreter, Variant system, stdlib
├── oxbasic_ui/          # Form model + event dispatcher + GPUI backend
└── oxbasic_ide/         # `oxbasic` CLI binary (run / gui / repl)
examples/                # hello.bas, demo.bas, fib.bas, clickme.bas + .oxform
scripts/                 # gui_smoke.sh — GUI launch smoke test
```

## Standard library (builtins)

Math: `Abs Sqr Int Fix Sgn Sin Cos Tan Atn Exp Log Rnd Randomize Hex Oct`
Strings: `Len Left Right Mid UCase LCase Trim LTrim RTrim InStr InStrRev
Replace Space String StrReverse Chr Asc Str Val Format Split Join StrComp IIf`
Conversions: `CInt CLng CDbl CSng CCur CVar CBool CStr CDate IsNumeric
IsArray IsEmpty IsNull IsDate VarType TypeName`
Arrays: `Array UBound LBound`
Dates: `Now Date Time Year Month Day Hour Minute Second Weekday DateSerial
TimeSerial DateAdd DateValue` (VB serial dates, day 0 = 1899-12-30)
Console: `Print`, `Debug.Print`, `MsgBox`, `InputBox`, `Beep`, `DoEvents`
Constants: `vbCrLf vbCr vbLf vbNewLine vbTab vbNullString vbOKOnly …`

## Building the GUI on macOS

The `gpui` feature needs Xcode's Metal toolchain once:

```sh
xcodebuild -downloadComponent MetalToolchain
```

## Known limitations

- GUI control *properties* (e.g. `Label1.Caption = "x"` from BASIC) are
  not yet writable from program code — the form renders from the
  `.oxform` spec; property binding arrives with the Phase 3 designer.
- `MsgBox` prints to the console attached to the GUI process (a native
  GPUI dialog ships with Phase 3).
- `Resume`/`Resume Next` resume precisely when the handler sits at the
  same block level as the failing statement (the common VB idiom).
- No `ReDim`, file I/O, or `Mid`-statement yet.

## Roadmap

- Phase 3: visual Form Designer, tree-sitter editor, property inspector,
  debugger
- Phase 4: Cranelift JIT backend and `.app` packaging
