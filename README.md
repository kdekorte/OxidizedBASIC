# OxidizedBASIC (`oxbasic`)

An open-source, ultra-fast, memory-safe classic BASIC programming suite
written entirely in Rust, per `oxidizedbasic_technical_specification.md`.
Full syntax and semantic compatibility with Visual Basic 6.0 / classic VBA
is the goal; GPUI-based GUI rendering and the visual IDE follow in later
phases.

## Status — Phases 1 & 2 complete, Phase 3 (IDE) in progress

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

### Phase 3: the visual IDE
- **`oxbasic ide [file.bas] [form.oxform]`** launches a GPUI IDE window:
  - **Code window** (View Code / F7, or double-click a control): opens
    the module in its own window. Its header has an object select box
    (`(General)`, `Form`, every control) and, beside it, the event select
    box for that object (bold = has code). Picking an event jumps to its
    handler, inserting a `Private Sub Object_Event(...)` placeholder if it
    doesn't exist. Double-clicking a control opens its default event.
  - **Code editor**: select with Shift + arrows (⌥ by word, ⌘ to the
    line or text ends) or the mouse (drag, Shift-click, double-click a
    word, triple-click a line); ⌘A selects all. Typing, Backspace and
    Delete replace the selection. **Cut / Copy / Paste** (⌘X ⌘C ⌘V) use
    the system clipboard, and **Undo / Redo** (⌘Z ⇧⌘Z) step through the
    edits (a run of typing is one step; an inserted event placeholder is
    one too). All are also on the new Edit menu.
  - **Visual form designer**: the form canvas renders the `.oxform`
    layout; click to select controls, **drag to move** them and **drag
    the eight selection handles** to resize them (like VB6), and add
    controls from the toolbox. With the form itself selected, handles
    on its bottom and right edges resize the form. The
    designer shows no code; Timers appear as ◷ icons.
  - **Property inspector**: `(Name)` (renames the control and its
    `Name_Event` handlers and references in code), Caption, Text, Items,
    Left/Top/Width/Height, Timer interval; the form's own properties show
    when nothing is selected. Names are validated as you type.

  - **Debugger**: Run / Step / Stop toolbar controls. The program runs
    on a background thread with a debug hook — **breakpoints by source
    line** (the parser now emits line markers), **step-over** semantics,
    pause/continue, and abort. While paused, the editor highlights the
    current line and the **Watch pane** shows every local and global
    variable with its value.
  - **Output pane** streams `Print`/`MsgBox` output live from the
    running program.
- **Runtime debugger API**: `Interpreter::set_debug_hook` /
    `set_breakpoints` with `PausedInfo` (line, procedure, depth, locals,
    globals) and `DebugAction` (Continue / StepOver / Abort) — usable
    headless (see the debugger integration tests).

## Usage

```sh
cargo build -p oxbasic_ide
./target/debug/oxbasic ide examples/clickme.bas examples/clickme.oxform  # the IDE
./target/debug/oxbasic run examples/demo.bas                    # console program
./target/debug/oxbasic gui examples/clickme.oxform \
                      examples/clickme.bas                      # GPUI form app
./target/debug/oxbasic repl                                     # immediate window
cargo test --workspace                                          # 52 tests
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
└── oxbasic_ide/         # `oxbasic` CLI binary (ide / gui / run / repl)
examples/                # hello.bas, demo.bas, fib.bas, clickme.bas + .oxform
scripts/                 # gui_smoke.sh, ide_smoke.sh — launch smoke tests
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

- **Control properties from BASIC** work when the program runs from the
  IDE (`oxbasic gui` and `oxbasic run` don't bind a form yet, so there
  these still fail). The program reads and writes the live form, and sees
  what the user typed, picked or clicked:
  - every control: `Caption`, `Text`, `Visible`, `Enabled`,
    `Left/Top/Width/Height`, `BackColor`, `ForeColor`, `Font.Name`,
    `Font.Size`, `Font.Bold/Italic/Underline/Strikethrough` (also
    `FontBold` etc.), `SetFocus`; the form: `Caption`, `Width`, `Height`
    through `Me` or its name;
  - `CheckBox.Value` (`vbUnchecked`/`vbChecked`/`vbGrayed`) and
    `OptionButton.Value` (True clears the form's other option buttons);
  - list and combo boxes: `AddItem [, index]`, `RemoveItem`, `Clear`,
    `List(i)`, `ListCount`, `ListIndex`;
  - timers: `Interval` and `Enabled` take effect on the running timer;
  - control arrays: give controls the same name and distinct `Index`
    values (Properties window); the program says `Cmd(2).Caption = "x"`
    and handlers take `Index As Integer`;
  - colours are VB `Long`s: `RGB(r, g, b)`, `vbRed`... Errors are VB's:
    438 unsupported member, 380 bad value, 381 bad list index, 340/343
    control-array element missing / not an array.
  Not yet: system colours, `Picture`, `ZOrder`, `Move`, `Container`
  (option buttons are one group per form), `Load`/`Unload` of array
  elements at run time.
- Run opens the form in its own OS window. Every event the code window
  lists reaches the program, in VB's order: mouse (`MouseDown`,
  `MouseMove`, `MouseUp` with button, shift and position; `Click` on
  release; `DblClick`), keys (`KeyDown`, `KeyPress`, `KeyUp`, with
  `KeyAscii`/`KeyCode` changeable and `KeyPreview`), focus (`GotFocus`,
  `LostFocus`, Tab), `Change`, `Scroll`, `DropDown`, `Timer`, and the
  form's `Load`, `Activate`, `Deactivate`, `Resize`, `Paint`,
  `QueryUnload` and `Unload`. Closing the window can be cancelled with
  `Cancel = True`. A property the program sets raises its event as in VB
  (`Text1.Text = "x"` runs `Text1_Change`). See `docs/controls.md`.
- Property-inspector editing and toolbox adding were reworked (per-field
  focus, redraw on add) but have only been checked by build/tests, not
  yet by hand.
- The IDE's editor has no find/replace, word wrap, or multiple carets;
  undo history is per document and is cleared when you switch documents
  or a rename changes the code. The debugger supports breakpoints,
  step-over, and watch (no step-into).
- `MsgBox` prints to the console/output pane (a native GPUI dialog is a
  follow-up).
- `Resume`/`Resume Next` resume precisely when the handler sits at the
  same block level as the failing statement (the common VB idiom).
- No `ReDim`, file I/O, or `Mid`-statement yet.

## Roadmap

- Phase 4: Cranelift JIT backend and `.app` packaging
