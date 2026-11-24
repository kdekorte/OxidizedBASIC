//! The runtime's member surface per control kind, checked against the member
//! tables the IDE's code model uses: a member a table lists must be handled here.

use crate::form::{ControlKind, ControlSpec, FormSpec};
use crate::host::FormHost;
use crate::Rect;
use oxbasic_runtime::{ObjectHost, ObjectRef};

const CANDIDATES: &[&str] = &[
    "name", "index", "left", "top", "width", "height", "visible", "enabled", "tabindex",
    "tabstop", "container", "tooltiptext", "mousepointer", "mouseicon", "move", "zorder",
    "forecolor", "backcolor", "font.name", "font.size", "font.bold", "caption", "text",
    "setfocus", "picture", "autosize", "currentx", "currenty", "cls", "line", "pset", "circle",
    "stretch", "selstart", "sellength", "seltext", "locked", "maxlength", "multiline",
    "scrollbars", "multiselect", "selected", "selcount", "list", "listcount", "listindex",
    "additem", "removeitem", "clear", "interval", "min", "max", "smallchange", "largechange",
    "value", "style", "shape", "fillstyle", "diagonal", "tab", "tabcaption", "tabs",
    "orientation", "path", "pattern", "filename", "filetitle", "drive", "volume",
    "checked", "alignment", "backstyle", "borderstyle", "default", "cancel",
    "defaultvalue", "rows", "cols", "row", "col", "toprow", "leftcol", "textmatrix",
    "fixedrows", "fixedcols", "editable", "colwidth", "rowheight", "scrollbars",
];

fn kinds() -> Vec<(&'static str, ControlKind)> {
    vec![
        ("CommandButton", ControlKind::CommandButton),
        ("Label", ControlKind::Label),
        ("TextBox", ControlKind::TextBox),
        ("CheckBox", ControlKind::CheckBox),
        ("OptionButton", ControlKind::OptionButton),
        ("Frame", ControlKind::Frame),
        ("ListBox", ControlKind::ListBox),
        ("ComboBox", ControlKind::ComboBox),
        ("PictureBox", ControlKind::PictureBox),
        ("Image", ControlKind::Image),
        ("Timer", ControlKind::Timer),
        ("HScrollBar", ControlKind::HScrollBar),
        ("Slider", ControlKind::Slider),
        ("ProgressBar", ControlKind::ProgressBar),
        ("Shape", ControlKind::Shape),
        ("Line", ControlKind::Line),
        ("TabControl", ControlKind::TabControl),
        ("DirListBox", ControlKind::DirListBox),
        ("FileListBox", ControlKind::FileListBox),
        ("VolumeListBox", ControlKind::VolumeListBox),
        ("DataGrid", ControlKind::DataGrid),
    ]
}

#[test]
fn the_members_the_member_check_lists_are_handled_by_the_runtime() {
    for (kind_name, kind) in kinds() {
        let mut f = FormSpec::new("Form1", "F", 400.0, 300.0);
        f.control(ControlSpec::new("Obj1", kind, Rect::new(0.0, 0.0, 100.0, 50.0)));
        let mut h = FormHost::new(f);
        let obj = ObjectRef { name: "Obj1".into(), index: None };
        let mut handled: Vec<&str> = vec![];
        for m in CANDIDATES {
            let via_get = match h.get(&obj, m, &[]) {
                Err(e) => e.number != 438,
                Ok(_) => true,
            };
            let via_call = match h.call(&obj, m, &[]) {
                Err(e) => e.number != 438,
                Ok(_) => true,
            };
            // a write: an assignment with a value, which the runtime handles
            // for read-write properties
            let via_set = match h.set(&obj, m, &[], oxbasic_runtime::Variant::Integer(1)) {
                Err(e) => e.number != 438,
                Ok(_) => true,
            };
            // an indexed property (ColWidth(c)) answers when given an index
            let via_indexed = match h.get(&obj, m, &[oxbasic_runtime::Variant::Integer(1)]) {
                Err(e) => e.number != 438,
                Ok(_) => true,
            };
            if via_get || via_call || via_set || via_indexed {
                handled.push(m);
            }
        }
        handled.sort();
        handled.dedup();
        // the members the runtime handles that the member check also needs:
        // a kind with a table must list every one of them (in code_model)
        let expected: &[&str] = match kind_name {
            "ListBox" => &["list", "listcount", "listindex", "additem", "removeitem", "clear", "selected", "selcount", "multiselect"],
            "ComboBox" => &["selstart", "sellength", "seltext", "locked", "selected", "list", "listcount", "listindex", "additem", "removeitem", "clear", "style"],
            "DataGrid" => &["scrollbars", "additem", "removeitem", "clear", "rows", "cols", "row", "col", "toprow", "leftcol", "textmatrix", "fixedrows", "fixedcols", "editable", "colwidth", "rowheight"],
            "TextBox" => &["selstart", "sellength", "seltext", "locked", "maxlength", "multiline", "scrollbars"],
            _ => &[],
        };
        for m in expected {
            assert!(handled.contains(m), "{} should handle {}", kind_name, m);
        }
        assert!(handled.len() > 0 || kind_name == "Label", "{} handles nothing", kind_name);
    }
}
