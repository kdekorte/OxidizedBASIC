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
| Timer | ◷ Timer | `Timer1` | none | — | — | — | no | no |

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
except Timer. Values outside `0..&HFFFFFF` raise 380.

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
| `SetFocus` | method | | yes | |

Typing in the window updates `Text` before `Change` fires.

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
| `SetFocus` | method | | yes | |

`ListIndex` is -1 when nothing is selected. Out-of-range indexes raise 381.

### ComboBox
| Member | Kind | Type | Run time | Design time |
|---|---|---|---|---|
| `Text` | property | String | R/W | R/W |
| `Style` | property | Integer | R/W | R/W |
| `List(i)`, `ListCount`, `ListIndex` | properties | | as ListBox | as ListBox |
| `AddItem`, `RemoveItem`, `Clear`, `SetFocus` | methods | | yes | |

`Style`: 0 Dropdown Combo, 1 Simple Combo, 2 Dropdown List. (Real VB makes
`Style` read-only at run time; here it can be set.) Setting `ListIndex`
copies the item into `Text`.

### PictureBox
No properties of its own beyond the common ones and `BackColor`. It takes
the focus (`SetFocus`, Tab, a click), so it gets keys, as in VB.

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

Setting or clicking one to True clears every other OptionButton on the
form (one group per form).

### Timer
| Member | Kind | Type | Run time | Design time |
|---|---|---|---|---|
| `Interval` | property | Long | R/W | R/W |
| `Enabled` | property | Boolean | R/W | R/W |

`Interval` is milliseconds; 0 or a disabled timer doesn't tick. Changes take
effect on the running timer immediately. Negative intervals raise 380.

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
| ListBox | `Click`, `DblClick`, `GotFocus`, `Key*`, `LostFocus`, `Mouse*`, `Scroll` |
| ComboBox | `Change`, `Click`, `DblClick`, `DropDown`, `GotFocus`, `Key*`, `LostFocus`, `Scroll` |
| PictureBox | `Click`, `DblClick`, `GotFocus`, `Key*`, `LostFocus`, `Mouse*`, `Paint`, `Resize` |
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

Not raised: a ListBox's `ItemCheck` and `Change`, and a PictureBox's
`Change`. VB raises these only for checkbox-style lists and for `Picture`,
which aren't supported, so the event box doesn't list them.

## Run-time errors

| Number | Meaning here |
|---|---|
| 5 | `SetFocus` on a disabled or invisible control |
| 340 | Control-array element doesn't exist (`Cmd(9)`) |
| 343 | Not an array (`Label1(0)`, `Cmd.Caption` on an array, `Index` of a plain control) |
| 380 | Invalid property value (size/colour/`Value`/`Style`/`Interval` out of range) |
| 381 | Invalid list index |
| 382 | Read-only property assigned (`ListBox.Text`) |
| 424 | Unknown object |
| 438 | Control doesn't have that property or method |
| 450 | Wrong number of arguments |

## Not supported yet

System colours, `Picture`, `Move`, `ZOrder`, `Container` (option buttons
are one group per form), `Load`/`Unload` of array elements at run time,
`ToolTipText`, `MousePointer`, `Locked`, `MaxLength`,
`MultiLine`, `ScrollBars`, `Selected()`/`MultiSelect`, `SelStart`/`SelText`.

