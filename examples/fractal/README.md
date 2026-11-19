# Fractal

A one-form sample that draws the Mandelbrot set pixel by pixel with `PSet`.

```
oxbasic ide examples/fractal/fractal.oxproj
```

Press **Run** (⌘R), then **Draw**. The picture fills in as it is drawn.

| What | Where |
|---|---|
| `picFractal` | a 640 × 480 PictureBox, empty until Draw is pressed |
| `cmdDraw` | the **Draw** button; its `Click` runs `DrawFractal` |
| `lblStatus` | shows the row being drawn, then the finished size |

The code in `Form1.bas` is a port of a QB64 demo (`SCREEN _NEWIMAGE(640, 480,
256)`, `PSET (X, Y), N`). The same escape-time loop runs; what changes is the
screen, which is the picture box here, and the colours:

* QB64's palette index `N` (0 to 255) goes through `Pal(N)`, which turns it into
  an RGB colour: index 0 (inside the set) is black and the rest ramp through the
  rainbow.
* `DoEvents` runs every 8 rows. It shows what has been drawn so far, and it is
  what lets **Stop** (or closing the window) end a draw in progress.
* `Draw` is disabled while a picture is drawing.

A full picture is 307,200 `PSet` calls. In a release build it takes about 20
seconds; a debug build is several times slower.

The demo's code is `Form1.bas`; the IDE only runs it, and the project file and layout are all the IDE sees.
