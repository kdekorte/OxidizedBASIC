# OxidizedBASIC controls reference

Every control the form designer offers, its properties, methods and events,
and whether BASIC code can **read only (R)** or **read/write (R/W)** each
one while the program runs.

This reflects the code as of commit `2d18558` (`crates/oxbasic_ui/src/host.rs`
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
| PictureBox | ▧ PictureBox | `Picture1` | 100x100 | — | — | — | no | no |
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
No properties of its own beyond the common ones and `BackColor`.

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

## Events

Handlers are `Sub <Name>_<Event>(...)`. **Fires** says whether the IDE's
live window raises it today; the others are listed in the code window's
event box (so a handler can be written) but nothing calls them yet. In a
control array each handler also gets `Index As Integer` first.

| Control | Fires today | Listed, not yet raised |
|---|---|---|
| CommandButton | `Click` | `GotFocus`, `KeyDown`, `KeyPress`, `KeyUp`, `LostFocus`, `MouseDown`, `MouseMove`, `MouseUp` |
| Label | `Click` | `Change`, `DblClick`, `MouseDown`, `MouseMove`, `MouseUp` |
| TextBox | `Change` (each keystroke), `Click` | `DblClick`, `GotFocus`, `KeyDown`, `KeyPress`, `KeyUp`, `LostFocus`, `Mouse*` |
| Frame | `Click` | `DblClick`, `Mouse*` |
| ListBox | `Click` (picking a row) | `Change`, `DblClick`, `GotFocus`, `ItemCheck`, `Key*`, `LostFocus`, `Mouse*`, `Scroll` |
| ComboBox | `Change`, `Click`, `DropDown` | `DblClick`, `GotFocus`, `Key*`, `LostFocus`, `Scroll` |
| PictureBox | `Click` | `Change`, `DblClick`, `GotFocus`, `Key*`, `LostFocus`, `Mouse*`, `Paint`, `Resize` |
| CheckBox, OptionButton | `Click` | `GotFocus`, `Key*`, `LostFocus`, `Mouse*` |
| Timer | `Timer` | |
| Form | `Load` | `Activate`, `Click`, `DblClick`, `Deactivate`, `Key*`, `Mouse*`, `Paint`, `QueryUnload`, `Resize`, `Unload` |

`Click` on a ListBox or ComboBox fires after the row is picked, so
`ListIndex` and `Text` already show it.

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
`TabIndex`/`TabStop`, `ToolTipText`, `MousePointer`, `Locked`, `MaxLength`,
`MultiLine`, `ScrollBars`, `Selected()`/`MultiSelect`, `SelStart`/`SelText`.

