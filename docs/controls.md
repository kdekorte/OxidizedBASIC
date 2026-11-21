# OxidizedBASIC controls reference

Every control the form designer offers, its properties, methods and events,
and whether BASIC code can **read only (R)** or **read/write (R/W)** each
one while the program runs.

This reflects the code as of the event-routing work after commit `2d18558` (`crates/oxbasic_ui/src/host.rs`
for run-time access, `crates/oxbasic_ui/src/lib.rs` for events,
`crates/oxbasic_ide/src/ide/mod.rs` for what the Properties window edits).

## Reading the tables

- **Run time** is what a BASIC program can do (`Label1.Caption = "x"`).
  This works when the program runs from the IDE; `oxbasic gui` and
  `oxbasic run` don't bind a form yet.
- **Design time** is whether the Properties window in the IDE edits it.
- **R** reads only; assigning raises error 438 (or 382 where noted).
  **R/W** reads and writes. **—** the control doesn't have it; both
  reading and writing raise error 438.
- Type is the BASIC type a read returns.

## Controls

| Control | Toolbox | Default name | Default size | Caption | Text | Value | Font/colours | Focus |
|---|---|---|---|---|---|---|---|---|
| CommandButton | OK CommandButton | `Command1` | 96x28 | yes | — | — | yes | yes |
| Label | Aa Label | `Label1` | 100x20 | yes | — | — | yes | no |
| TextBox | ab TextBox | `Text1` | 120x24 | — | yes | — | yes | yes |
| Frame | ☐ Frame | `Frame1` | 160x100 | yes | — | — | yes | no |
| ListBox | ≡ ListBox | `List1` | 120x84 | — | read-only | — | yes | yes |
| ComboBox | ⌄ ComboBox | `Combo1` | 120x24 | — | yes | — | yes | yes |
| PictureBox | ▧ PictureBox | `Picture1` | 100x100 | — | — | — | no | yes |
| CheckBox | ☑ CheckBox | `Check1` | 110x22 | yes | — | 0/1/2 | yes | yes |
| OptionButton | ◉ OptionButton | `Option1` | 110x22 | yes | — | True/False | yes | yes |
| Image | ▣ Image | `Image1` | 100x100 | — | — | — | no | no |
| Timer | ◷ Timer | `Timer1` | none | — | — | — | no | no |
| HScrollBar | ↔ HScrollBar | `HScroll1` | 120x17 | — | — | Min..Max | no | yes |
| VScrollBar | ↕ VScrollBar | `VScroll1` | 17x120 | — | — | Min..Max | no | yes |
| Line | ╱ Line | `Line1` | 120x60 | — | — | — | yes (colour) | no |
| Shape | ▭ Shape | `Shape1` | 100x60 | — | — | — | yes (colour) | no |
| VolumeListBox | ▤ VolumeListBox | `Volume1` | 120x84 | — | read-only | — | yes | yes |
| DirListBox | ▥ DirListBox | `Dir1` | 120x84 | — | read-only | — | yes | yes |
| FileListBox | ▦ FileListBox | `File1` | 120x84 | — | read-only | — | yes | yes |
| Slider | ⇆ Slider | `Slider1` | 160x40 | — | — | Min..Max | no | yes |
| ProgressBar | ▰ ProgressBar | `Progress1` | 200x20 | — | — | Min..Max | no | no |
| TabControl | ▤ TabControl | `Tab1` | 320x200 | — | — | Tab | yes | yes |

"Font/colours" means `ForeColor` and `Font.*`. `BackColor` applies to
everything except Timer. "Focus" means `SetFocus` works.

## Properties common to every control

| Property | Type | Run time | Design time | Notes |
|---|---|---|---|---|
| `Name` | String | R | R/W | Renaming rewrites `Name_Event` handlers and code references. |
| `Index` | Integer | R | R/W | Set a number to put the control in an array; blank leaves it. Reading on a non-array raises 343. |
| `Left` | Single | R/W | R/W | Form pixels. Not offered for a Timer in the Properties window; setting it at run time does nothing visible. |
| `Top` | Single | R/W | R/W | |
| `Width` | Single | R/W | R/W | Must be above 0 (else 380). |
| `Height` | Single | R/W | R/W | Must be above 0 (else 380). |
| `Enabled` | Boolean | R/W | R/W | A disabled control is greyed and can't take focus. |
| `Visible` | Boolean | R/W | — | Not in the Properties window. A Timer is always invisible. |
| `TabIndex` | Integer | R/W | R/W | Place in the tab order, from 0; every control but a Timer has one. Setting it moves the control there and the ones after it down one, as in VB. Out of range raises 380. |
| `TabStop` | Boolean | R/W | R/W | Tab skips the control when False (a click still focuses it). Only controls that take the focus. |
| `Container` | object | R/W (`Set`) | R/W | The Frame or PictureBox it sits in (reads as its name; the form's name when on the form). `Set Text1.Container = Frame1` moves it in, `= Me` back out; `Left`/`Top` then count from the container. A hidden or disabled container hides or disables what it holds. Anything but a Frame/PictureBox, or a container inside the control itself, raises 380. In the designer, dropping a control on a Frame puts it in. |
| `ToolTipText` | String | R/W | R/W | Shown after the pointer rests on the control for half a second. |
| `MousePointer` | Integer | R/W | R/W | `vbArrow` 1, `vbCrosshair` 2, `vbIbeam` 3, `vbSizeNESW` 6 … `vbHourglass` 11 (the macOS waiting cursor), `vbNoDrop` 12, `vbArrowHourglass` 13 (also the waiting cursor), `vbCustom` 99 (the control's `MouseIcon`). Others raise 380. |
| `MouseIcon` | picture | R/W | R/W (a file name) | The image shown as the pointer when `MousePointer` is 99, from `LoadPicture`. Scaled down to fit 32 by 32 pixels, its top-left corner the hot spot. A file that isn't an image, or no icon, shows the pointing hand. |
| `Move left [, top [, width [, height]]]` | method | yes | — | Sets those given; a size change raises `Resize` as `Width` would. |
| `ZOrder [position]` | method | yes | — | 0 (`vbBringToFront`, the default) draws it over the others, 1 (`vbSendToBack`) under them. The tab order is kept. |

System colours: every colour property also takes VB's system colours
(`vbButtonFace`, `vbWindowBackground`, `vbInfoBackground`… the
`&H800000nn` values). They read back as set, and are drawn in this IDE's
light look. In the Properties window they are typed and shown as
`&H8000000F&`.

## Properties of the text-showing controls

All controls except PictureBox and Timer.

| Property | Type | Run time | Design time | Notes |
|---|---|---|---|---|
| `ForeColor` | Long | R/W | R/W | VB colour (`&H00BBGGRR`): `RGB(r,g,b)`, `vbRed`... Design time takes `#RRGGBB` or `&HBBGGRR`; blank is the usual colour. |
| `Font.Name` (`FontName`) | String | R/W | R/W | Reads `MS Sans Serif` when unset (the system font is used). |
| `Font.Size` (`FontSize`) | Single | R/W | R/W | Points, 1 to 2048 (else 380). |
| `Font.Bold` (`FontBold`) | Boolean | R/W | R/W | |
| `Font.Italic` (`FontItalic`) | Boolean | R/W | R/W | |
| `Font.Underline` (`FontUnderline`) | Boolean | R/W | R/W | |
| `Font.Strikethrough` (`FontStrikethru`) | Boolean | R/W | — | Not in the Properties window. |

`BackColor` (Long, R/W run time, R/W design time) applies to every control
except Timer. Values outside `0..&HFFFFFF` that aren't a system colour raise
380.

## Properties and methods per control

### CommandButton
| Member | Kind | Type | Run time | Design time |
|---|---|---|---|---|
| `Caption` | property | String | R/W | R/W |
| `SetFocus` | method | | yes | |

### Label
| Member | Kind | Type | Run time | Design time |
|---|---|---|---|---|
| `Caption` | property | String | R/W | R/W |

### TextBox
| Member | Kind | Type | Run time | Design time |
|---|---|---|---|---|
| `Text` | property | String | R/W | R/W |
| `SelStart` | property | Long | R/W | — |
| `SelLength` | property | Long | R/W | — |
| `SelText` | property | String | R/W | — |
| `Locked` | property | Boolean | R/W | R/W |
| `MaxLength` | property | Long | R/W | R/W |
| `MultiLine` | property | Boolean | R | R/W |
| `ScrollBars` | property | Integer | R | R/W |
| `SetFocus` | method | | yes | |

Typing in the window updates `Text` before `Change` fires.

- **Selection.** `SelStart` is the caret (in characters, from 0) and
  `SelLength` the selected characters after it; setting `SelStart` drops the
  selection, `SelLength` extends it from `SelStart`. `SelText` reads the
  selection, and setting it replaces the selection (or inserts at the
  caret). In the window, Shift with the arrows, Home and End selects, and
  typing replaces the selection. Negative values raise 380.
- **`Locked`**: the text can't be changed by typing (the program still can).
- **`MaxLength`**: typing, `Text` and `SelText` stop at that many characters;
  0 is no limit.
- **`MultiLine`** and **`ScrollBars`** (0 none, 1 horizontal, 2 vertical,
  3 both) are set at design time, as in VB; assigning them raises 382. A
  MultiLine box takes Enter as a new line, moves with Up/Down, and wraps
  lines unless it has a horizontal scroll bar.
- **Mouse.** A click puts the caret between the characters nearest the
  pointer, measured in the box's own font. Dragging selects the text it
  crosses, and keeps selecting while the pointer is outside the box; a
  Shift-click extends the selection; a double click selects the word
  under it (a run of letters, digits and `_`, a run of spaces, or one
  other character); a triple click selects the line, or all of a
  one-line box. The program sees the result in `SelStart`/`SelLength`.

### Frame
| Member | Kind | Type | Run time | Design time |
|---|---|---|---|---|
| `Caption` | property | String | R/W | R/W |

### ListBox
| Member | Kind | Type | Run time | Design time |
|---|---|---|---|---|
| `List(i)` | property | String | R/W | R/W as one comma-separated `List` row |
| `ListCount` | property | Long | R | — (follows `List`) |
| `ListIndex` | property | Long | R/W | — |
| `Text` | property | String | R: the selected row (assigning raises 382) | — |
| `AddItem item [, index]` | method | | yes | |
| `RemoveItem index` | method | | yes | |
| `Clear` | method | | yes | |
| `Selected(i)` | property | Boolean | R/W | — |
| `SelCount` | property | Long | R | — |
| `MultiSelect` | property | Integer | R | R/W |
| `SetFocus` | method | | yes | |

`ListIndex` is -1 when nothing is selected. Out-of-range indexes raise 381.

`MultiSelect` (design time only; assigning raises 382): 0 one row, 1
(`vbMultiSelectSimple`) a click or Space toggles a row, 2
(`vbMultiSelectExtended`) a click selects one row, Shift-click a range and
⌘/Ctrl-click toggles one. `Selected(i)` says whether row `i` is selected,
and selects it; in a one-row list it is the same as `ListIndex = i`.
`ListIndex` is the row with the focus.

### ComboBox
| Member | Kind | Type | Run time | Design time |
|---|---|---|---|---|
| `Text` | property | String | R/W | R/W |
| `Style` | property | Integer | R/W | R/W |
| `List(i)`, `ListCount`, `ListIndex` | properties | | as ListBox | as ListBox |
| `SelStart`, `SelLength`, `SelText`, `Locked` | properties | | as TextBox | `Locked` R/W |
| `AddItem`, `RemoveItem`, `Clear`, `SetFocus` | methods | | yes | |

`Style`: 0 Dropdown Combo, 1 Simple Combo, 2 Dropdown List. (Real VB makes
`Style` read-only at run time; here it can be set.) Setting `ListIndex`
copies the item into `Text`.

### PictureBox
| Member | Kind | Type | Run time | Design time |
|---|---|---|---|---|
| `Picture` | property | picture | R/W | R/W (a file name) |
| `AutoSize` | property | Boolean | R/W | R/W |
| `CurrentX`, `CurrentY` | properties | Single | R/W | — |
| `Cls` | method | | yes | |
| `Line`, `PSet`, `Circle` | statements | | yes | |
| `SetFocus` | method | | yes | |

`Picture = LoadPicture("logo.png")` shows the image at its own size from
the top-left corner (as VB does without `AutoSize`); `LoadPicture()` clears
it. Relative file names are in the project's folder. A file that isn't
there raises 53. Here a picture is its file name, so `Picture` reads as
that name. Changing it raises `Change`.

It takes the focus (`SetFocus`, Tab, a click), so it gets keys, and it can
hold other controls (`Container`), as in VB.

`AutoSize` makes the box the picture's size plus its 2-pixel border on each
side, as the picture or the setting changes (a missing file leaves it).

**Drawing.** `Line`, `PSet` and `Circle` draw on a PictureBox; on the form
itself, a Frame or an Image they raise 438.

    Picture1.Line (10, 10)-(100, 60), vbRed        ' a line
    Picture1.Line (10, 10)-(100, 60), , B          ' a box (BF fills it)
    Picture1.Line -(150, 20)                       ' on from where it ended
    Picture1.Line -Step(0, 30)                     ' Step counts from there
    Picture1.PSet (50, 50), RGB(0, 128, 0)         ' one dot
    Picture1.Circle (80, 80), 30, vbBlue           ' a circle
    Picture1.Circle (80, 80), 30, , 0, 3.14159, 0.5  ' a half ellipse
    Picture1.Cls                                   ' clear it

Coordinates are pixels from the box's top-left. A colour left out is the
control's `ForeColor`; colours are VB Longs, system colours included. A
negative `start` or `end` in `Circle` also draws the radius to it (a pie
slice); `aspect` is height over width. `CurrentX`/`CurrentY` are where
the last shape ended (a circle: its centre) and `Cls` resets them. Shapes
are drawn over the picture in the order they were drawn, and a program
keeps at most 20,000 of them (the oldest go). `PSet` is the exception: it
sets a pixel of a bitmap the size of the box (clipped to it, drawn under
the shapes), so any number of pixels can be set, as in a fractal; `Cls`
clears the bitmap too.

`DoEvents` shows what the program has drawn so far, so a long drawing
appears as it goes; Stop (or closing the window) ends it. Clicks and keys
that arrive while a handler is busy are not delivered.

`Line`, `PSet` and `Circle` are not reserved words: they are drawing
statements only when a point `(x, y)` follows, so a variable called
`Line` still works.

### Image
| Member | Kind | Type | Run time | Design time |
|---|---|---|---|---|
| `Picture` | property | picture | R/W | R/W (a file name) |
| `Stretch` | property | Boolean | R/W | R/W |

VB's lightweight picture: no border, no focus, no drawing, and it can't
hold other controls. With `Stretch` the picture is scaled to the control's
size; without it, it is drawn at its own size from the top-left corner.
(`Stretch` belongs to the Image in VB, not the PictureBox.)

### CheckBox
| Member | Kind | Type | Run time | Design time |
|---|---|---|---|---|
| `Caption` | property | String | R/W | R/W |
| `Value` | property | Integer | R/W | R/W |
| `SetFocus` | method | | yes | |

`Value`: 0 `vbUnchecked`, 1 `vbChecked`, 2 `vbGrayed`; other values raise 380.
A click toggles 0 and 1.

### OptionButton
| Member | Kind | Type | Run time | Design time |
|---|---|---|---|---|
| `Caption` | property | String | R/W | R/W |
| `Value` | property | Boolean | R/W | R/W (0 or 1) |
| `SetFocus` | method | | yes | |

Setting or clicking one to True clears the other OptionButtons of its group:
those in the same container (the same Frame or PictureBox, or the form
itself), as in VB.

### Timer
| Member | Kind | Type | Run time | Design time |
|---|---|---|---|---|
| `Interval` | property | Long | R/W | R/W |
| `Enabled` | property | Boolean | R/W | R/W |

`Interval` is milliseconds; 0 or a disabled timer doesn't tick. Changes take
effect on the running timer immediately. Negative intervals raise 380.

### HScrollBar and VScrollBar
Both are a slider over `Min`..`Max`. The thumb is at `Value`.

| Member | Kind | Type | Run time | Design time |
|---|---|---|---|---|
| `Min` | property | Long | R/W | R/W |
| `Max` | property | Long | R/W | R/W |
| `Value` | property | Long | R/W | R/W |
| `SmallChange` | property | Long | R/W | R/W |
| `LargeChange` | property | Long | R/W | R/W |

The defaults are 0, 32767, 0 and 1. `Min` must stay below `Max`; `Value`
is kept inside the range, and changing `Min` or `Max` moves `Value` into the
new range. Out-of-range values raise 380. `SmallChange` and `LargeChange`
must be at least 1.

Dragging the thumb, or pressing on the track, sets `Value` and raises
`Change`, whose handler runs straight away. Setting `Value` in code raises
`Change` too, as VB does.

### Line
| Member | Kind | Type | Run time | Design time |
|---|---|---|---|---|
| `Diagonal` | property | Boolean | R/W | R/W |

A line runs from the control's top-left to its bottom-right corner, or from
bottom-left to top-right when `Diagonal` is True. Its colour is `ForeColor`.
`Width` and `Height` set its length, so resizing a Line from code moves it.

### Shape
| Member | Kind | Type | Run time | Design time |
|---|---|---|---|---|
| `Shape` | property | Integer | R/W | R/W |
| `FillStyle` | property | Integer | R/W | R/W |

`Shape`: 0 rectangle, 1 square, 2 oval, 3 circle (a square or circle takes
the shorter of the control's sides). `FillStyle`: 0 solid, 1 transparent
(the default). Its outline and fill use `ForeColor`. Values outside these
ranges raise 380.

### VolumeListBox
A list of the mounted volumes, as `/Volumes` shows them, sorted. It reads the
list when the form loads. It works as a ListBox does (`List`, `ListCount`,
`ListIndex`, `Text`, `Click`), but its items come from the disk, so `AddItem`
and `RemoveItem` change the list only until the form is reloaded.

### DirListBox
A list of the sub-folders of `Path`, sorted, with hidden entries left out.

| Member | Kind | Type | Run time | Design time |
|---|---|---|---|---|
| `Path` | property | String | R/W | R/W |

`Path` is the folder listed. When it is empty, the first mounted volume is
listed. Setting `Path` to something that isn't a folder raises 76 (Path not
found) and leaves the list as it was. Setting `Path` lists that folder and
raises `Change`. Pair it with a VolumeListBox by setting `Path` to
`"/Volumes/" & Volume1.Text` in the volume's `Click`.

### FileListBox
A list of the regular files in `Path` whose names match `Pattern`, sorted,
with hidden files and folders left out.

| Member | Kind | Type | Run time | Design time |
|---|---|---|---|---|
| `Path` | property | String | R/W | R/W |
| `Pattern` | property | String | R/W | R/W |
| `FileName` | property | String | R | — |

`Pattern` is one or more patterns separated by `;`, such as `*.bas;*.oxproj`.
Each is `*.ext` (names ending in `ext`) or an exact name, ignoring case. An
empty pattern lists every file. Setting `Path` or `Pattern` reloads the list.
A `Path` that isn't a folder raises 76.

`FileName` is the selected file's full path (`Path` and the file's name), or
an empty string when nothing is selected. The same pattern rule is what the
file dialogs use (see `OpenFileDialog` in the README).

### Slider
A draggable thumb over `Min`..`Max`, read and set through `Value`.

| Member | Kind | Type | Run time | Design time |
|---|---|---|---|---|
| `Min` | property | Long | R/W | R/W |
| `Max` | property | Long | R/W | R/W |
| `Value` | property | Long | R/W | R/W |
| `Orientation` | property | Integer | R/W | R/W |

`Orientation` is 0 (horizontal, the default) or 1 (vertical); other values
raise 380. The range and `Value` follow the scrollbars' rules: `Min` stays
below `Max`, and `Value` is kept inside the range. Dragging the thumb sets
`Value` and raises `Change`; setting `Value` in code raises `Change` too.

### ProgressBar
A bar filled from `Min` to `Value` over `Min`..`Max`. The user can't drag it:
only the program sets `Value` (and `Min`/`Max`), and out-of-range values raise
380. It raises no events.

### TabControl
A row of tabs with a page under them. The controls on a page sit inside the
TabControl (`Set Text1.Container = Tab1`) and are shown only while that page
is selected. A control that is on another page is hidden and takes no input,
as one in a hidden Frame does.

| Member | Kind | Type | Run time | Design time |
|---|---|---|---|---|
| `Tab` | property | Integer | R/W | R/W |
| `Tabs` | property | Integer | R (the number of tabs) | — |
| `TabCaption` | property | String | R (the selected tab's caption, `""` if none) | — |

`Tab` is the selected page, from 0 up to the last tab; other values raise 380.
Setting `Tab` shows that page and raises `Click`, as VB's TabStrip does. The
tab captions are set in the designer.

### Menu
A form's menu bar is not a control: it is the form's `menus` list. Each item
has a `name` (its handler is `name_Click`), a
`caption` (`&File`; a `-` is a separator), a `parent` (the item it sits
under; none for the bar), a `shortcut` (`Ctrl+O`, `Ctrl+Shift+S`, `F5`), and
`enabled` and `checked` flags.

- While the form's window is active, its menu is the application's menu bar,
  and the IDE's menu comes back when the window is not in front.
- A menu's caption drops the `&` mnemonic marker, because a Mac menu has no
  mnemonics: the shortcut is drawn to the right of the item instead. `&&` is a
  literal ampersand.
- Choosing an item runs its `Name_Click` handler in the program. A disabled
  item and a separator don't run.
- A shortcut runs the item it belongs to. Ctrl and Cmd both mean Command on
  the Mac.
- A menu's item and handler names must be unique; a submenu's parent must
  exist, and a menu can't contain itself (the editor refuses such moves).

### Form
Reached as `Me` or by its own name (`Form1.Caption`).

| Member | Type | Run time | Design time |
|---|---|---|---|
| `Name` | String | R | R/W |
| `Caption` | String | R/W | R/W |
| `Width` | Single | R/W | R/W |
| `Height` | Single | R/W | R/W |
| `KeyPreview` | Boolean | R/W | R/W |

`KeyPreview` makes the form's key handlers run before the focused control's
(see *Keys* below); it is also saved in the `.oxform` file.

## Events

Handlers are `Sub <Name>_<Event>(...)`. Every event the code window's
event box lists is raised in the IDE's live window. In a control array each
handler also gets `Index As Integer` first. Arguments are passed by
reference, as in VB: a handler may set `KeyAscii`, `KeyCode` or `Cancel`.

| Control | Events |
|---|---|
| CommandButton | `Click`, `GotFocus`, `KeyDown`, `KeyPress`, `KeyUp`, `LostFocus`, `MouseDown`, `MouseMove`, `MouseUp` |
| Label | `Change`, `Click`, `DblClick`, `MouseDown`, `MouseMove`, `MouseUp` |
| TextBox | `Change`, `Click`, `DblClick`, `GotFocus`, `KeyDown`, `KeyPress`, `KeyUp`, `LostFocus`, `Mouse*` |
| Frame | `Click`, `DblClick`, `Mouse*` |
| Image | `Click`, `DblClick`, `Mouse*` |
| ListBox | `Click`, `DblClick`, `GotFocus`, `Key*`, `LostFocus`, `Mouse*`, `Scroll` |
| VolumeListBox | `Click`, `DblClick`, `GotFocus`, `Key*`, `LostFocus`, `Mouse*` |
| DirListBox | `Change`, `Click`, `DblClick`, `GotFocus`, `Key*`, `LostFocus`, `Mouse*` |
| FileListBox | `Click`, `DblClick`, `GotFocus`, `Key*`, `LostFocus`, `Mouse*` |
| HScrollBar | `Change`, `GotFocus`, `LostFocus`, `Mouse*` |
| VScrollBar | `Change`, `GotFocus`, `LostFocus`, `Mouse*` |
| Line, Shape | `Mouse*` |
| Slider | `Change`, `GotFocus`, `LostFocus`, `Mouse*` |
| ProgressBar | none (the program sets it) |
| TabControl | `Click`, `Mouse*` (a tab click changes the page) |
| ComboBox | `Change`, `Click`, `DblClick`, `DropDown`, `GotFocus`, `Key*`, `LostFocus`, `Scroll` |
| PictureBox | `Change`, `Click`, `DblClick`, `GotFocus`, `Key*`, `LostFocus`, `Mouse*`, `Paint`, `Resize` |
| CheckBox, OptionButton | `Click`, `GotFocus`, `Key*`, `LostFocus`, `Mouse*` |
| Timer | `Timer` |
| Form | `Activate`, `Click`, `DblClick`, `Deactivate`, `Key*`, `Load`, `Mouse*`, `Paint`, `QueryUnload`, `Resize`, `Unload` |

`Mouse*` is `MouseDown`, `MouseMove`, `MouseUp`; `Key*` is `KeyDown`,
`KeyPress`, `KeyUp`.

### When they are raised

- **Mouse.** `Button` is 1 (left), 2 (right) or 4 (middle); `Shift` is
  1 Shift + 2 Ctrl (⌘ on a Mac) + 4 Alt; `X` and `Y` are in pixels from the
  control's own top-left (from the form's for the form). The control a
  button went down on gets every `MouseMove` and the `MouseUp` until it
  comes up, even outside it. A press where there is no control, or on a
  disabled one, is the form's.
- **Click order.** `GotFocus` (if the control takes the focus), `MouseDown`,
  then on release `Click` and `MouseUp`. `Click` only comes from the left
  button, released over the control it was pressed on. A double click is
  `MouseDown`, `Click`, `MouseUp`, `DblClick`, `MouseUp`. Clicks in a quick
  burst count in pairs, as in Windows: click, double, click, double. A list
  row is picked, and its `Click` raised, on the press.
- **Keys.** `KeyDown` (`KeyCode` is VB's key code, `vbKeyA` = 65 whatever
  the case), `KeyPress` for keys that type a character (`KeyAscii`: `a` is
  97, Enter 13), the control's `Change`, then `KeyUp` on release. Setting
  `KeyAscii` to 0 swallows the character; another value types that one
  instead. Setting `KeyCode` to 0 cancels a key that types nothing (an
  arrow, Delete) and skips the handlers after it. With the form's
  `KeyPreview` set, the form's handler runs before the control's for each.
- **Focus.** `LostFocus` of the control losing it, then `GotFocus` of the
  new one: on a click, on `SetFocus`, and on Tab / Shift-Tab, which move
  through the controls in `TabIndex` order (the order they were added until
  it is changed), skipping those with `TabStop` False, disabled or
  invisible ones. Labels, frames and timers take no focus.
- **Window.** `Activate` / `Deactivate` when the form's window becomes or
  stops being the active one; `Resize` and `Paint` when it opens and when
  it is resized (the program reads the new `Width` and `Height`), and
  `Paint` again each time the window is activated, which is how a window
  that was covered comes back to the front. Closing
  the window raises `QueryUnload(Cancel, UnloadMode)` with `UnloadMode` 0,
  then `Unload(Cancel)`; either can set `Cancel` to keep the window. The
  `Unload` statement raises the same pair with `UnloadMode` 1.
- **From the program's own changes.** Setting a property raises its event
  inside the statement, before the next line runs, as in VB: `Text` of a
  TextBox or ComboBox and a Label's `Caption` raise `Change`; `ListIndex`
  raises `Click`; a CheckBox's `Value` raises `Click`, as does setting an
  OptionButton to `True`; a PictureBox's or the form's size raises
  `Resize`. Setting a property to the value it has raises nothing.

`Click` on a ListBox or ComboBox fires after the row is picked, so
`ListIndex` and `Text` already show it.

Not raised: a ListBox's `ItemCheck` and `Change`. VB raises these only for
checkbox-style lists (`Style` 1), which aren't supported, so the event box
doesn't list them.

## Control arrays at run time

`Load Cmd(n)` adds element `n` to control array `Cmd`: a copy of its
lowest-numbered element as designed, hidden (`Visible = False`) until the
program shows it, with an empty text and selection. It raises the array's
events with its `Index`. `Unload Cmd(n)` removes it again. Loading an
element that exists raises 360, unloading one placed in the designer 362,
an element that doesn't exist 340; `Load` of something that isn't an array
343.

## Run-time errors

| Number | Meaning here |
|---|---|
| 5 | `SetFocus` on a disabled or invisible control (or one in a disabled or hidden container); `ZOrder` other than 0 or 1 |
| 53 | `LoadPicture` of a file that isn't there |
| 340 | Control-array element doesn't exist (`Cmd(9)`) |
| 343 | Not an array (`Label1(0)`, `Cmd.Caption` on an array, `Index` of a plain control, `Load Label1(1)`) |
| 360 | `Load` of an array element that is loaded already |
| 362 | `Unload` of an array element placed in the designer |
| 76 | `Path` of a DirListBox or FileListBox that isn't a folder |
| 380 | Invalid property value (size/colour/`Value`/`Min`/`Max`/`SmallChange`/`Style`/`Interval`/`TabIndex`/`MousePointer`/`SelStart`/`Shape`/`FillStyle` out of range, a `Container` that can't hold the control) |
| 381 | Invalid list index |
| 382 | Read-only property assigned (`ListBox.Text`; `MultiLine`, `ScrollBars` and `MultiSelect` at run time) |
| 424 | Unknown object |
| 438 | Control doesn't have that property or method |
| 450 | Wrong number of arguments |

## Not supported yet

Drawing on the form itself, a Frame or an Image; `DrawWidth`, `DrawStyle`,
`FillColor`, `ScaleMode` and `Point()`; `Print` onto a
PictureBox. Checkbox-style lists (`ListBox.Style` 1), `Sorted`,
`ItemData`, `TopIndex`, `ListBox.Columns`. `DragDrop`/`OLEDrag`. Animated
or `.cur` cursors (a `MouseIcon` is a still image). On a platform other
than macOS the waiting cursor and `MouseIcon` show the arrow and the
pointing hand. The Menu Editor (Properties → Menu Editor…) edits the items,
their shortcuts (modifier toggles and a key, F1–F12 included), and their order.
Shapes are not clamped to the form, so a Shape or Line sized
past the form's edge is drawn past it. `FileListBox.Pattern` raises no
`Change`, and `FileName` can't be set. Choosing a file doesn't open it.

