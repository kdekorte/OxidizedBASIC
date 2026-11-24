# Cranelift JIT: design and measurements

Phase 4 of the technical specification compiles BASIC routines to native code
with Cranelift. This note fixes the first step's scope and records the numbers
it has to beat.

## Measurements

Both are the same 10,000,000-iteration loop as `examples/basica.bas`: an
`If` on `I MOD 2`, with `COUNT` accumulating `I` or `-I`.

| Version | Time | Result |
|---|---|---|
| Interpreter (`oxbasic run examples/basica.bas`) | 1.29 s user | `-5000000` |
| Native Rust, same loop (`black_box` on the counter) | 6.5 ms | `-5000000` |

The interpreter is about 200 times slower than native code for this loop.

## First step: numeric `For` loops only

The JIT compiles one construct: a `For` loop whose body uses only

- numeric scalar variables (`Long`, `Integer`, `Double`, `Single`) that are
  locals of the loop's procedure or the main module,
- numeric literals and arithmetic (`+ - * / \ MOD ^`), comparisons and
  `And`/`Or`/`Not`,
- `If ... Then ... Else` and nested `For` loops over the same kinds of body.

Anything else in the body (a call, a string, an array, a control, a file, an
`Exit`, a `GoTo`) means the loop is **not compiled**; the interpreter runs it
exactly as now. This keeps the compiled code free of the IDE's objects, the
frame's name map and the Variant types it cannot represent.

Compiled code must give the same answers as the interpreter, including VB's
integer overflow errors and the loop's final counter value. Where a case is not
proven to match, the loop falls back to the interpreter.

## Out of scope for the first step

- Procedures, parameters and `ByRef` (the frame and slot work comes first).
- Strings, arrays, objects, `With` and event handlers.
- Any change to the interpreter's behaviour when the JIT is off.

## Acceptance

- The compiled `basica.bas` loop gives `-5000000` and the same final counter.
- Every existing runtime test passes with the JIT on and off.
- A loop the JIT cannot compile runs unchanged in the interpreter.
- The timing against the table above is recorded here.

## Prototype result

A standalone Cranelift program (outside the repository) compiles this exact
loop to native code: `i` runs from 0 to the argument, `count` adds `i` when
`i mod 2 = 0` and subtracts it otherwise, and the function returns `count`.

| Version | Time (3 runs) | Result |
|---|---|---|
| Interpreter (`oxbasic run examples/basica.bas`) | 1.29 s user | `-5000000` |
| Cranelift JIT, prototype | 5.6 to 7.9 ms | `-5000000` |
| Native Rust (`black_box`) | 6.5 ms | `-5000000` |

So the JIT reaches native speed for this loop, roughly 180 times faster than the
interpreter. This is the loop's compiled cost only: it does not include reading
the variables from the interpreter's frame or writing the results back, which the
next step must add and measure.

## Still to do

- Connect the compiler to the runtime: recognise a qualifying `For` loop, read
  its variables from the frame, run the compiled code and write the results back.
- Prove the compiled loop gives the interpreter's answers for every case the
  first step covers (the integer `MOD` sign for negative numbers, overflow, and
  the final counter value).
- Measure `basica.bas` end to end with the JIT on, and record it here.

