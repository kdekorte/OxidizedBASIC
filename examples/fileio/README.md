# File I/O example

A form project showing VB's three kinds of file: **sequential**, **random** and
**binary**. Open `fileio.oxproj` in the IDE, or package it with `oxbasic play`.
The form has a button for each demo, and a box that shows what the demo found.

```
oxbasic ide examples/fileio/fileio.oxproj
```

The demos write their files next to the project, so `scores.txt`, `notes.txt`,
`people.dat` and `raw.bin` appear in this folder. Each run rewrites them, except
that *Sequential files* appends a record each time it runs.

## Layout

| File | What it is |
|---|---|
| `fileio.oxproj` | The project: one form, one module |
| `frmRecords.oxform` | The form: buttons and the output box |
| `frmRecords.bas` | The form's code: each button runs a demo |
| `Module1.bas` | The demos: `SequentialDemo`, `RandomDemo`, `BinaryDemo` |

The demos live in a standard module, which a form's handlers can call. A module
cannot reach a form's controls, so each demo returns its report as text, and the
form shows it.

## What it shows

| Statement or function | Used for |
|---|---|
| `Open … For Output As #n` | Create or replace a sequential file |
| `Open … For Append As #n` | Add to the end of a sequential file |
| `Open … For Input As #n` | Read a sequential file |
| `Write #n, …` | Write records: text quoted, values comma-separated |
| `Print #n, …` | Write a line of text |
| `Input #n, a, b` | Read comma-separated values into variables |
| `Line Input #n, s` | Read one whole line |
| `EOF(n)` | True once the last line has been read |
| `On Error GoTo` | Handle a file that is not there (error 53) |
| `Type … End Type` | A record: fields in order, with a `Type` as a field too |
| `String * n` | A fixed-length text field, exactly `n` bytes in the file |
| `Open … For Random As #n Len = size` | Fixed-length records of `size` bytes |
| `Put #n, recnum, value` / `Get #n, recnum, var` | Write and read a record |
| `Open … For Binary As #n` | Raw bytes, read and written at a position |
| `Seek #n, pos` | Move to a 1-based position (a byte, in a binary file) |
| `LOF(n)` | The file's length in bytes |
| `Close #n` | Close a file |

## Expected output

*Sequential files*:

```
Ada scored 91 and 88.5
Grace, H scored 77 and 94
Linus scored 85 and 79
Records: 3
Average of the first scores: 84.33333333333333
1: Scores are out of 100.
2: Alan asked for a re-mark.
Error 53: File not found: no_such_file.txt
```

*Random records*:

```
Records: 3
3: Person3 age 23 at Main St 3 10003
2: Person2 age 22 at Main St 2 10002
1: Person1 age 21 at Main St 1 10001
```

*Binary bytes*:

```
Text: HELLO
Number: 258
Length: 7
```

The numbers here have no spaces around them, because the report joins its
values with `&`. A `Print` of a number adds VB's spaces, so `Print` shows
`Error  53` with two spaces.

## The file layouts

Random and binary files use VB's binary layout, so a record a program writes
can be read by VB and the other way round:

- A `Type` is its fields in order with no padding between them.
- `String * n` is exactly `n` bytes, padded with spaces.
- `Integer` is 2 bytes, `Long` 4, `Single` 4, `Double` 8, `Currency` 8
  (scaled by 10,000), `Date` 8, `Boolean` 2 (`-1` or `0`).
- A dynamic `String` is a 2-byte length then its characters.
- A random record is padded to its `Len =` size. A record longer than that is
  an error (63), not cut short.

So `people.dat` holds 32 bytes per person, and `raw.bin` is `HELLO` (5 bytes)
followed by the Integer `258` as `2 1` (7 bytes in all).

## Notes

- A binary `Get` into a dynamic `String` reads that string's current length, so
  this example uses a fixed-length string (`String * 5`) as the buffer.
- The layouts above are the ones VB documents. A file written by real VB has not
  yet been compared byte for byte.
