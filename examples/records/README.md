# Random and binary file example

A standalone program that writes and reads VB's random and binary files. Run it
from this folder, since the files are written next to the program:

```
cd examples/records
oxbasic run records.bas
```

It writes `people.dat` (fixed-length records) and `raw.bin` (bytes), reads them
back and prints what it found. Both files are rewritten each run.

## What it shows

| Statement or function | Used for |
|---|---|
| `Type … End Type` | A record: fields in order, with a `Type` as a field too |
| `String * n` | A fixed-length text field, exactly `n` bytes in the file |
| `Open … For Random As #n Len = size` | Fixed-length records of `size` bytes |
| `Put #n, recnum, value` | Write a record (padded to `size`) |
| `Get #n, recnum, var` | Read a record back into a variable |
| `Open … For Binary As #n` | Raw bytes, read and written at a position |
| `Seek #n, pos` | Move to a 1-based position (a byte, in a binary file) |
| `LOF(n)` | The file's length in bytes |

## Expected output

```
Records:  3
 3 : Person3 age  23  at Main St 3  10003
 2 : Person2 age  22  at Main St 2  10002
 1 : Person1 age  21  at Main St 1  10001
Text: HELLO
Number:  258
Length:  7
Done
```

## The file layout

The bytes follow VB's binary layout, so a record a program writes can be read by
VB and the other way round:

- A `Type` is its fields in order with no padding between them.
- `String * n` is exactly `n` bytes, padded with spaces.
- `Integer` is 2 bytes, `Long` 4, `Single` 4, `Double` 8, `Currency` 8
  (scaled by 10,000), `Date` 8, `Boolean` 2 (`-1` or `0`).
- A dynamic `String` is a 2-byte length then its characters.
- A random record is padded to its `Len =` size. A binary write is exactly the
  bytes of its value, with no padding.

So `people.dat` holds 32 bytes per person, and `raw.bin` is `HELLO` (5 bytes)
followed by the Integer `258` as `2 1` (7 bytes in all).

## Notes

- A binary `Get` into a dynamic `String` reads that string's current length, so
  VB programs use a fixed-length string (`String * n`) as the buffer, as this
  example does.
- The layouts above are the ones VB documents. A file written by real VB 6 has
  not yet been compared byte for byte.
