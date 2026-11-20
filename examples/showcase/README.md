# Control showcase

A two-form sample project that uses every control the toolbox offers.

```
oxbasic ide examples/showcase/showcase.oxproj
```

Then press **Run** (⌘R). The project panel lists `Form1` (the start-up form),
`frmAbout` and `Module1`.

| Try this | What it shows |
|---|---|
| Type a name, press **Greet** | TextBox `Text`, Label `Caption`, `SetFocus` |
| Tick **Shout it**; switch **Formal / Casual** | CheckBox and OptionButton `Value`; choosing one option clears the other |
| Pick a **Colour** | ComboBox `ListIndex` / `Text`; PictureBox `BackColor`, `RGB()`, `vbRed` |
| **Add fruit**, **Remove**, **Clear list**, click a row | ListBox `AddItem`, `RemoveItem`, `Clear`, `ListCount`, `ListIndex`, `Text` |
| **Swatch 1 / 2 / 3** | A control array: one `cmdSwatch_Click(Index)` handler, `cmdSwatch(i).Font.Bold` |
| **Start / stop clock** | Timer `Enabled`, `Timer` events |
| **About…** | A second form: `frmAbout.Show`, writing to its controls, its own `Form_Load` |
| About's **Hide** / **Close** | `Me.Hide` keeps the form's state; `Unload Me` drops it |
| **Quit** | `End` finishes the whole program |
| Drag the **width** and **height** scrollbars | HScrollBar / VScrollBar `Value` and `Change`; the Shape's and Line's `Width` and `Height` follow |
| Pick a **volume**, then a **folder** | VolumeListBox `Text`, DirListBox `Path` (a `Change` when it moves) |
| Pick a **file** in the list | FileListBox `Pattern` (`*.bas;*.oxproj`), `ListIndex`, and `FileName` (the full path) |

`Module1.bas` holds `Plural` and `Describe`, which both forms can call.
Each form's own handlers and `Dim` variables are private to it, so two forms can
both have a `Command1_Click`.

Tests run this exact project (`cargo test -p oxbasic_ide showcase`), so it
stays working. If you change a layout in `crates/oxbasic_ide/src/ide/showcase.rs`,
regenerate the files with `OXBASIC_WRITE_SHOWCASE=1 cargo test -p oxbasic_ide regenerate_when_asked`.
