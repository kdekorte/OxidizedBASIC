# OxidizedBASIC (`oxbasic`)

An open-source, ultra-fast, memory-safe classic BASIC programming suite
written entirely in Rust, per `oxidizedbasic_technical_specification.md`.
Full syntax and semantic compatibility with Visual Basic 6.0 / classic VBA
is the goal; GPUI-based GUI rendering and the visual IDE follow in later
phases.

## Status — Phase 1 complete

- **Parser** (`oxbasic_parser`): `pest` grammar for the classic VB6/VBA
  statement surface — `If/Then/ElseIf/Else` (block + single-line),
  `Select Case` (values, `To` ranges, `Is` comparators, `Case Else`),
  `For…Next [Step]`, `For Each…In`, `Do While/Until…Loop` (pre- and
  post-test), `While…Wend`, `Sub`/`Function` with `ByVal`/`ByRef`/
  `Optional`/`ParamArray`, `Dim`/`Static`/`Const`, arrays, labels,
  `GoTo`, `On Error GoTo <label>|0`, `On Error Resume Next`, `Resume [Next|label]`,
  `Exit {Sub,Function,For,Do}`, `End`, `Option Explicit`, `Print`/`Debug.Print`
  with `;`/`,` separators, colon-separated statements, `&H`/`&O` literals,
  `'` and `Rem` comments, and full VB operator precedence
  (`^` > unary `-` > `* /` > `\` > `Mod` > `+ -` > `&` > comparisons >
  `Not` > `And` > `Or` > `Xor` > `Eqv` > `Imp`).
- **Runtime** (`oxbasic_runtime`): tree-walking interpreter with a
  VB-style `Variant` tagged enum (`Empty`, `Null`, `Boolean`, `Integer`,
  `Long`, `Single`, `Double`, `Currency`, `Date`, `String`, arrays),
  `Rc<RefCell<…>>`-shared scopes so `ByRef` parameters mutate callers,
  multi-dimension fixed arrays plus auto-growing dynamic arrays,
  trappable `On Error` handling with `Err.Number`/`Err.Description`/
  `Err.Clear`/`Err.Raise`, and a standard library (string, math, date,
  conversion, array, and console functions — see below).
- **UI bridge** (`oxbasic_ui`): the classic-control → GPUI Kit mapping
  table from spec §5 plus the render-agnostic event dispatcher registry
  that Phase 2 will wire to real GPUI components.
- **CLI** (`oxbasic`): run programs, interactive immediate window (REPL),
  and the IDE launcher placeholder.

## Usage

```sh
cargo run -p oxbasic_ide -- run examples/demo.bas   # run a program
cargo run -p oxbasic_ide -- repl                    # immediate window
cargo test --workspace                              # 41 tests
```

REPL: type statements, or `? <expr>` to print an expression value.

## Workspace layout

```
crates/
├── oxbasic_ast/         # AST data structures & symbol definitions
├── oxbasic_parser/      # Grammar (pest), lexer, parser
├── oxbasic_runtime/     # Interpreter, Variant system, stdlib
├── oxbasic_ui/          # GPUI & GPUI Kit widget mapping (Phase 2 bridge)
└── oxbasic_ide/         # `oxbasic` CLI binary (IDE app in Phase 3)
examples/                # hello.bas, demo.bas, fib.bas
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

## Known Phase 1 limitations

- Object/COM members (`Form1.Caption = …`) raise a trappable "object
  required" error; they arrive with the Phase 2 GPUI bridge.
- `Resume`/`Resume Next` resume precisely when the handler sits at the
  same block level as the failing statement (the common VB idiom of a
  handler at the end of a procedure).
- No `ReDim`, file I/O, or `Mid`-statement yet.

## Roadmap

- Phase 2: GPUI element binding engine (`oxbasic_ui` on real GPUI)
- Phase 3: visual Form Designer, tree-sitter editor, property inspector,
  debugger
- Phase 4: Cranelift JIT backend and `.app` packaging
