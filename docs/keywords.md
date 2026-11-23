# OxidizedBASIC keyword reference

Every keyword the language accepts, what it does, and the limits that matter.
Keywords and identifiers are case-insensitive: `print`, `PRINT` and `Print`
are the same statement. Each line is one statement, or several separated by
`:`. A `'` or `Rem` starts a comment.

This follows the grammar (`crates/oxbasic_parser/src/oxbasic.pest`) and the
runtime, so it describes what runs today. A line the parser cannot read is
reported as a syntax error, and the editor underlines it.

## Program structure

| Keyword | Form | Meaning |
|---|---|---|
| `Sub` | `[Public\|Private] Sub Name(params)` … `End Sub` | A procedure with no value. |
| `Function` | `[Public\|Private] Function Name(params) [As Type]` … `End Function` | A procedure that returns a value: assign the value to `Name`. |
| `Static` | `Static x` | Declares a variable that keeps its value between calls. |
| `Global` | `Global x` | Declares a module-level variable. |
| `Public`, `Private`, `Friend` | Visibility of a procedure | Accepted. The runtime does not restrict access. |
| `Exit` | `Exit Sub`, `Exit Function`, `Exit For`, `Exit Do` | Leaves the named block early. |
| `End` | `End` | Stops the program at once. |
| `Stop` | `Stop` | Pauses in the debugger. Does nothing outside the IDE. |

Parameters may be `ByVal` or `ByRef` (ByRef is the VB default), `Optional`
with a default value, `ParamArray` for a variable list, and `As Type`. A
`ByRef` argument shares the caller's variable.

## Declarations

| Keyword | Form | Meaning |
|---|---|---|
| `Dim` | `Dim x`, `Dim x As Long`, `Dim a(10)`, `Dim a()` | Declares a variable, a fixed array (indices `0` to `10`, or `1` to `10` under `Option Base 1`), or a dynamic array (`a()`). |
| `Const` | `Const N = 10` | A named constant. |
| `ReDim` | `ReDim a(n)`, `ReDim Preserve a(n)` | Resizes a dynamic array. `Preserve` keeps the elements that are still in range. |
| `Erase` | `Erase a` | Resets a fixed array to its defaults, or empties a dynamic array. |
| `Let` | `Let x = 5` | Optional: the same as `x = 5`. |
| `Set` | `Set obj = Form2` | Assigns an object. |
| `Type` | `Type Point` … `End Type` | A record. Each line is `Field As Type`. A field may be an array (`Vals(2) As Long`) or another `Type`. |
| `Enum` | `Enum Color` … `End Enum` | Named constants. A member without `= value` is one more than the member before it, starting at 0. |
| `DefInt`, `DefLng`, `DefStr`, `DefBool`, `DefDbl`, `DefSng`, `DefCur`, `DefDate`, `DefVar`, `DefObj`, `DefVrnt` | `DefInt I-N, X` | The type given to undeclared names whose first letter falls in each range. An explicit `As` type wins. |

Declared types are `Integer`, `Long`, `Single`, `Double`, `Currency`,
`String`, `Boolean`, `Date`, `Variant`, and any user `Type`. A numeric
variable starts at `0`, a `String` starts empty, and a `Boolean` starts
`False`.

## Option statements

| Keyword | Form | Meaning |
|---|---|---|
| `Option Explicit` | `Option Explicit` | Every variable must be declared before it is used. |
| `Option Base` | `Option Base 0` or `Option Base 1` | The lowest index of arrays declared after it. |
| `Option Compare` | `Option Compare Text` or `Option Compare Binary` | `Text` makes string comparisons ignore case. |

## Control flow

| Keyword | Form | Meaning |
|---|---|---|
| `If` | `If c Then …`, `If c Then … [ElseIf …] [Else …] End If`, `If c Then s1 Else s2` | Conditional. Block and one-line forms are both supported. |
| `Select Case` | `Select Case x` / `Case 1, 2` / `Case 3 To 5` / `Case Is > 9` / `Case Else` / `End Select` | A multi-way choice. |
| `For` | `For i = 1 To 10 [Step 2]` … `Next [i]` | A counted loop. |
| `For Each` | `For Each v In coll` … `Next [v]` | A loop over the items of an array. |
| `Do` | `Do While c` … `Loop`, `Do` … `Loop Until c` | A loop with the condition before or after the body. |
| `While` | `While c` … `Wend` | Loops while the condition holds. |
| `GoTo` | `GoTo label` | Jumps to a label in the same procedure. |
| `GoSub` | `GoSub label` … `Return` | Calls a subroutine inside the procedure. `Return` goes back to the statement after the `GoSub`. A `Return` with no `GoSub` is error 3. |
| `Return` | `Return` | Ends a `GoSub` subroutine. |
| `Call` | `Call Name(args)` | Calls a procedure. The parentheses are optional when there are no arguments. |
| `On Error` | `On Error GoTo label`, `On Error GoTo 0`, `On Error Resume Next` | Error handling. An error that is not trapped stops the program and reports its number, its description and the line it happened on (`runtime error 11: division by zero on line 3`). |
| `Resume` | `Resume`, `Resume Next`, `Resume label` | Continues after an error handler. |
| `With` | `With obj` … `End With` | Inside the block, a leading `.Member` is a member of `obj`, so `.Caption = "x"` sets `obj.Caption`. Blocks may nest. |

## Statements

| Keyword | Form | Meaning |
|---|---|---|
| `Print` | `Print a; b, c` | Writes values. `;` joins them, and `,` pads to the next column. A number is printed with a space before and after it. |
| `Debug.Print` | `Debug.Print x` | Writes to the Immediate pane in the IDE. |
| `Mid` | `Mid(s, start [, length]) = text` | Replaces characters of a string in place. The text never extends past the end of the string. |
| `Line`, `PSet`, `Circle` | `Line (x1,y1)-(x2,y2), color, B`, `PSet (x,y), c`, `Circle (x,y), r` | Drawing on a form or a PictureBox. `Step` makes the coordinates relative to the last point. |

## Operators

| Operator | Meaning |
|---|---|
| `^` | Power. |
| `-`, `+` (unary) | Sign. |
| `*`, `/` | Multiply, divide. |
| `\` | Integer division. |
| `Mod` | Remainder. |
| `+`, `-` | Add, subtract. |
| `&` | Join strings. |
| `=`, `<>`, `<`, `<=`, `>`, `>=`, `Is`, `Like` | Comparison. String comparison follows `Option Compare`. |
| `Not`, `And`, `Or`, `Xor`, `Eqv`, `Imp` | Logical and bitwise operators, in VB's order of precedence. |

Literals: decimal (`10`, `1.5`, `1E3`), hexadecimal (`&HFF`), octal (`&O17`),
strings in double quotes (write `""` for a quote inside one), and the
constants `True`, `False`, `Empty`, `Null`, `Nothing`, and the `vb…`
constants such as `vbCrLf`.

## Built-in functions

Math: `Abs`, `Sgn`, `Int`, `Fix`, `Sqr`, `Sin`, `Cos`, `Tan`, `Atn`, `Exp`,
`Log`, `Rnd`, `Randomize`, `Round` (rounds half to even), `Timer`.

Strings: `Len`, `Left`, `Right`, `Mid`, `InStr`, `InStrRev`, `Replace`,
`UCase`, `LCase`, `Trim`, `LTrim`, `RTrim`, `Space`, `String`, `StrReverse`,
`Chr`, `Asc`, `Str`, `Val`, `StrComp`, `Split`, `Join`, `Hex`, `Oct`.

Choice: `IIf`, `Choose`, `Switch`.

Conversion: `CInt`, `CLng`, `CDbl`, `CSng`, `CCur`, `CVar`, `CBool`, `CStr`,
`CDate`.

Type tests: `IsNumeric`, `IsArray`, `IsEmpty`, `IsNull`, `IsDate`, `VarType`,
`TypeName`.

Arrays: `Array(…)`, `LBound`, `UBound`. Both follow `Option Base`.

Dates and time: `Now`, `Date`, `Time`, `Year`, `Month`, `Day`, `Weekday`,
`Hour`, `Minute`, `Second`, `DateSerial`, `TimeSerial`, `DateAdd`,
`DateValue`. A date is a serial number: whole days since 1899-12-30, with the
time of day as the fraction.

Formatting: `Format(value, pattern)`. Named formats are `General Number`,
`Currency`, `Fixed`, `Standard`, `Percent`, `Scientific`, `Yes/No`,
`True/False` and `On/Off`. Numeric patterns use `0` (a digit), `#` (a digit
if one is there), `,` (thousands separators), `.` (the decimal point), `%`
(multiply by 100), and `E+` or `E-` (scientific notation). Date patterns use
`yyyy`, `yy`, `mmmm`, `mmm`, `mm`, `m`, `dddd`, `ddd`, `dd`, `d`, `hh`, `nn`,
`ss`, `q` (quarter), `w` (weekday number), `AM/PM` and `A/P`, with quoted or
`\`-escaped literal text.

Dialogs and the interface: `MsgBox`, `InputBox`, `OpenFileDialog`,
`SaveFileDialog`, `DoEvents`, `Beep`, `LoadPicture`.

## File I/O

| Statement | Form | Meaning |
|---|---|---|
| `Open` | `Open path For Input\|Output\|Append As #n` | Opens a file under number `n` (1 to 511). `Output` replaces the file, `Append` adds to it. |
| `Close` | `Close`, `Close #n [, #m]` | Closes the files named; bare `Close` closes them all. Files still open at the end of the program are closed then. |
| `Input #` | `Input #n, a, b` | Reads comma-separated values into variables. A quoted value keeps its commas. |
| `Line Input #` | `Line Input #n, s` | Reads one whole line into a variable. |
| `Print #` | `Print #n, a; b` | Writes a line, laid out as `Print` lays it out. |
| `Write #` | `Write #n, "text", 3` | Writes a line with strings quoted and items comma-separated, so `Input #` can read it back. |
| `EOF` | `EOF(n)` | True once the last line of file `n` has been read. |
| `Open … For Random` | `Open path For Random As #n [Len = size]` | Fixed-length records, `size` bytes each (default 128). |
| `Open … For Binary` | `Open path For Binary As #n` | Raw bytes, read and written at a position. |
| `Get` | `Get #n, [recnum], var` | Reads a record (random) or `var`-sized bytes at the position (binary). |
| `Put` | `Put #n, [recnum], value` | Writes a record, padded to the record length. |
| `Seek` | `Seek #n, pos` | Moves to a 1-based position: a record number (random), a byte (binary). |
| `Loc` | `Loc(n)` | Random: the last record number. Binary: the next byte position. |
| `Lof` | `Lof(n)` | The file's length in bytes. |

Relative file names are in the program's folder (or the project's, from the
IDE). Errors: 52 bad file number, 53 file not found, 54 wrong use for the file's
mode, 55 file already open, 62 input past the end of the file, 63 bad record
number.

Records and bytes are stored as text, padded to the record length. This is not
VB's binary layout: a file written here is not byte-compatible with one written
by VB, and `Get` returns text, so numbers come back as strings.


- `Property Get`, `Property Let` and `Property Set`. A procedure written with
  them is a syntax error. (`Exit Property` is recognised.)
- Classes, and `New` for anything other than the built-in controls and `Err`.
## Not supported

- File I/O is sequential, random and binary. Not supported: `Lock`/`Unlock`,
  and the file functions `FreeFile` and `LOF` on a sequential file.
- `Step Into` in the debugger. Step Over is supported.
- `Option Base` values other than `0` and `1`.


