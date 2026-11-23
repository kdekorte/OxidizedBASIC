# File I/O example

A standalone program that uses VB's sequential files. Run it from this folder:

```
oxbasic run examples/files/files.bas
```

It writes two files next to the program, `scores.txt` and `notes.txt`, reads
them back, and prints what it found. Running it again starts over (the files
are rewritten), except that `AppendScore` adds a record each time.

## What it shows

| Statement or function | Used for |
|---|---|
| `Open … For Output As #n` | Create (or replace) a file |
| `Open … For Append As #n` | Add to the end of a file |
| `Open … For Input As #n` | Read a file |
| `Write #n, …` | Write records: text quoted, values comma-separated |
| `Print #n, …` | Write a line of text |
| `Input #n, a, b` | Read comma-separated values into variables |
| `Line Input #n, s` | Read one whole line |
| `EOF(n)` | True once the last line has been read |
| `Close #n` | Close a file (the program also closes any still open at the end) |
| `On Error GoTo` | Handle a file that is not there (error 53) |

## Expected output

```
Ada scored  91  and  88.5
Grace, H scored  77  and  94
Linus scored  85  and  79
Records:  3
Average of the first scores:  84.33333333333333
 1 : Scores are out of 100.
 2 : Alan asked for a re-mark.
Error  53 : File not found: no_such_file.txt
Done
```

Numbers print with a space either side, as VB prints them, so `Error  53`
has two spaces. `Grace, H` keeps its comma, because a quoted field is one
value. The record file is `"Ada",91,88.5` and so on, one record a line.

## File errors

| Number | When |
|---|---|
| 52 | A file number outside 1 to 511, or a number not open |
| 53 | An input file that does not exist |
| 54 | Reading a file opened for output, or writing one opened for input |
| 55 | A number that is already open |
| 62 | Reading past the end of the file |

File names are relative to the folder the program runs from, or to the
project's folder when the program runs from the IDE.
