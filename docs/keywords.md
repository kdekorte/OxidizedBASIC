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

### Math

| Function | Returns | What it does |
|---|---|---|
| `Abs(x)` | Same type as `x` | The absolute value. |
| `Sgn(x)` | Long | `1` if `x` is positive, `-1` if negative, `0` if zero. |
| `Int(x)` | Number | The largest whole number not above `x`: `Int(-2.5)` is `-3`. |
| `Fix(x)` | Number | `x` with the fraction removed, toward zero: `Fix(-2.5)` is `-2`. |
| `Sqr(x)` | Double | The square root. Error 5 when `x` is negative. |
| `Sin(x)`, `Cos(x)`, `Tan(x)`, `Atn(x)` | Double | Sine, cosine, tangent and arctangent, in radians. |
| `Exp(x)` | Double | e raised to `x`. |
| `Log(x)` | Double | The natural logarithm. Error 5 when `x` is not positive. |
| `Round(x [, digits])` | Number | `x` rounded to `digits` decimal places (default 0). A number exactly halfway goes to the even neighbour: `Round(2.5)` is `2`, `Round(3.5)` is `4`. |
| `Rnd` | Double | A pseudo-random number from 0 up to, not including, 1. |
| `Randomize [seed]` | (statement) | Seeds `Rnd`. With no seed it uses the clock. The same seed gives the same sequence. |
| `Timer` | Single | Seconds since midnight. |
| `Hex(n)` | String | `n` in hexadecimal, capitals: `Hex(255)` is `FF`. |
| `Oct(n)` | String | `n` in octal: `Oct(8)` is `10`. |

### Strings

| Function | Returns | What it does |
|---|---|---|
| `Len(s)` | Long | The number of characters in `s`. |
| `Left(s, n)` | String | The first `n` characters. |
| `Right(s, n)` | String | The last `n` characters. |
| `Mid(s, start [, n])` | String | `n` characters from position `start` (1-based), or to the end when `n` is left out. Error 5 when `start` is below 1. |
| `InStr([start,] s, find)` | Long | The position of `find` in `s` (1-based), searched from `start`; `0` when not found. With `start` the search begins there. |
| `InStrRev(s, find)` | Long | The position of the last `find` in `s`; `0` when not found. |
| `Replace(s, find, repl)` | String | Every `find` in `s` replaced by `repl`. |
| `UCase(s)`, `LCase(s)` | String | `s` in capitals or in lower case. |
| `Trim(s)`, `LTrim(s)`, `RTrim(s)` | String | `s` without spaces at both ends, at the start, or at the end. |
| `Space(n)` | String | `n` spaces. |
| `String(n, c)` | String | `n` copies of the character `c` (a string or a character code). |
| `StrReverse(s)` | String | `s` backwards. |
| `Chr(code)` | String | The character with that code. Error 5 for an invalid code. |
| `Asc(s)` | Long | The code of the first character of `s`. Error 5 for an empty string. |
| `Str(n)` | String | `n` as text. A non-negative number gets a leading space, as `Print` gives it: `Str(5)` is ` 5`, `Str(-5)` is `-5`. |
| `Val(s)` | Double | The number at the start of `s`, ignoring leading spaces and stopping at the first character that is not part of a number: `Val("3.5 apples")` is `3.5`. `0` when there is none. |
| `StrComp(a, b)` | Long | `-1`, `0` or `1` as `a` sorts before, equal to, or after `b`. |
| `Split(s [, delim])` | Array | `s` cut at each `delim` (a space by default) into a zero-based array of pieces. |
| `Join(array [, delim])` | String | The array's items joined by `delim` (a space by default). |
| `Hex`, `Oct` | String | See Math. |

### Choice

| Function | Returns | What it does |
|---|---|---|
| `IIf(cond, a, b)` | Either | `a` when `cond` is true, otherwise `b`. Both are evaluated. |
| `Choose(i, v1, v2, ...)` | Either | The `i`th value (1-based). Null when `i` is outside the list. |
| `Switch(c1, v1, c2, v2, ...)` | Either | The value after the first true condition. Null when none is true. |

### Conversion

| Function | Returns | What it does |
|---|---|---|
| `CInt(x)`, `CLng(x)` | Integer, Long | `x` rounded to a whole number. A half goes to the even neighbour, as `Round` does. |
| `CDbl(x)`, `CSng(x)`, `CCur(x)`, `CVar(x)` | Double | `x` as a number. |
| `CBool(x)` | Boolean | `False` for zero, `True` for anything else. |
| `CStr(x)` | String | `x` as text, without the leading space that `Str` adds. |
| `CDate(x)` | Date | `x` as a date serial. A string is not parsed here: use `DateValue`. |

### Type tests

| Function | Returns | What it does |
|---|---|---|
| `IsNumeric(x)` | Boolean | True for a number, or for a string that reads as one. |
| `IsArray(x)` | Boolean | True for an array. |
| `IsEmpty(x)` | Boolean | True for a variable that has never been given a value. |
| `IsNull(x)` | Boolean | True for `Null`. |
| `IsDate(x)` | Boolean | True for a date value (not for a string). |
| `VarType(x)` | Long | The type code: 0 Empty, 1 Null, 2 Integer, 3 Long, 4 Single, 5 Double, 6 Currency, 7 Date, 8 String, 9 Object, 11 Boolean, 8204 Array. |
| `TypeName(x)` | String | The type's name: `Long`, `String`, `Boolean`, `Empty` and so on. |

### Arrays

| Function | Returns | What it does |
|---|---|---|
| `Array(v1, v2, ...)` | Array | An array of the values given, starting at the lower bound (`Option Base`). |
| `LBound(a [, dim])` | Long | The first index of the array (per dimension). |
| `UBound(a [, dim])` | Long | The last index of the array (per dimension). Error 9 when the dimension does not exist. |

### Dates and time

A date is a serial number: whole days since 1899-12-30, with the time of day as
the fraction. So `Now` is a date with a time, and `Date` is `Now` with the time
dropped.

| Function | Returns | What it does |
|---|---|---|
| `Now` | Date | The current date and time. |
| `Date` | Date | Today's date, with no time. |
| `Time` | Date | The current time of day, as a fraction of a day. |
| `Year(d)`, `Month(d)`, `Day(d)` | Long | The parts of a date. |
| `Weekday(d)` | Long | The day of the week, 1 for Sunday to 7 for Saturday. |
| `Hour(d)`, `Minute(d)`, `Second(d)` | Long | The parts of a time. |
| `DateSerial(y, m, d)` | Date | The date for a year, month and day. |
| `TimeSerial(h, m, s)` | Date | The time of day as a fraction of a day. |
| `DateAdd(unit, n, d)` | Date | `d` moved by `n` units. Units: `yyyy` years, `m` months (the day is kept where it exists), `d`, `y` or `w` days, `ww` weeks, `h` hours, `n` minutes, `s` seconds. An unknown unit is error 5. |
| `DateValue(s)` | Date | The date a string holds, such as `"2024-03-05"`. |

### Formatting

`Format(value, pattern)` writes a value in a form. The pattern is either a named
format or a picture made of the codes below.

Named numeric formats:

| Name | Shows |
|---|---|
| `General Number` | The number as it is. |
| `Currency` | Thousands separators and two decimals: `1234.5` is `1,234.50`. No currency sign is added. |
| `Fixed` | Two decimals, no separators: `3` is `3.00`. |
| `Standard` | Thousands separators and two decimals: `1234.5` is `1,234.50`. |
| `Percent` | Times 100 with a `%` and two decimals: `0.5` is `50.00%`. |
| `Scientific` | Scientific notation with two decimals. |
| `Yes/No`, `True/False`, `On/Off` | The words for a number: zero gives the second word, non-zero the first (`Format(12, "Yes/No")` is `Yes`). |

Picture codes for numbers:

- `0` a digit, always shown (padded with zeros).
- `#` a digit, shown only if there is one.
- `,` thousands separators.
- `.` the decimal point.
- `%` multiplies by 100 and shows `%`.
- `E+` or `E-` scientific notation, as in `0.0E+00`.

Picture codes for dates: `yyyy`, `yy` (year); `mmmm`, `mmm`, `mm`, `m` (month
name, short name, two digits, one digit); `dddd`, `ddd`, `dd`, `d` (day name,
short name, two digits, one digit); `hh`, `nn`, `ss` (hour, minute, second);
`q` (quarter); `w` (weekday number); `AM/PM` and `A/P` (a 12-hour clock). Quote
literal text, or escape a character with a backslash.

### Dialogs and the interface

| Function or statement | What it does |
|---|---|
| `MsgBox(prompt)` | Writes `prompt` and a line break to the output, and returns `1`. |
| `InputBox(prompt)` | Writes `prompt` and reads one line from the input, which it returns. |
| `OpenFileDialog(filter)`, `SaveFileDialog(filter)` | Show the file dialog. A cancelled dialog, or a file the filter rejects, gives `""`. |
| `DoEvents` | Lets the window and pending events run. |
| `Beep` | A statement that does nothing here. |
| `LoadPicture(file)` | Loads a picture for a PictureBox's `Picture`. |

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
| `Get` | `Get #n, [recnum], var` | Reads a record (random) into `var`, decoding each field from VB's binary layout. |
| `Put` | `Put #n, [recnum], value` | Writes a record in VB's binary layout, padded to the record length. |
| `Seek` | `Seek #n, pos` | Moves to a 1-based position: a record number (random), a byte (binary). |
| `Loc` | `Loc(n)` | Random: the last record number. Binary: the next byte position. |
| `Lof` | `Lof(n)` | The file's length in bytes. Works for sequential files too: an output file's buffered text is counted. |
| `Lock` | `Lock #n [, record \| , from To to]` | Locks a file, or the records in a range. A range that overlaps one already locked is error 70, Permission denied. Locks are released when the file is closed. |
| `Unlock` | `Unlock #n [, record \| , from To to]` | Releases a lock made with the same range. Unlocking a range that is not locked is error 70. |
| `FreeFile` | `FreeFile` or `FreeFile()` | The lowest file number (1 to 511) not in use. Error 67 when all are in use. |

Relative file names are in the program's folder (or the project's, from the
IDE). Errors: 52 bad file number, 53 file not found, 54 wrong use for the file's
mode, 55 file already open, 62 input past the end of the file, 63 bad record
number.

Records use VB's binary layout. A `Type` is its fields in order with no padding,
each in its VB form: `Integer` 2 bytes, `Long` 4, `Single` 4, `Double` 8,
`Currency` 8 (scaled by 10,000), `Date` 8, `Boolean` 2 (`-1` or `0`), a
`String * n` field exactly `n` bytes space-padded, and a dynamic `String`
length-prefixed. A nested `Type` is laid out field by field. The record is then
padded to the `Len =` length. `Get` decodes the bytes back into the variable,
with each field's own type. Layouts were checked against the byte sequences VB
documents; a file written by real VB is not yet compared.


- `Property Get`, `Property Let` and `Property Set`. A procedure written with
  them is a syntax error. (`Exit Property` is recognised.)
- Classes, and `New` for anything other than the built-in controls and `Err`.
## Not supported

- File I/O is sequential, random and binary. Every file statement and function
  in the table above is supported.
- `Step Into` in the debugger. Step Over is supported.
- `Option Base` values other than `0` and `1`.


