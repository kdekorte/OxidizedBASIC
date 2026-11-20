//! The render-agnostic form model: classic VB forms and controls with
//! their layout coordinates, properties, and `.oxform` JSON layout
//! serialization (spec §6), plus the event dispatch that connects UI
//! events to BASIC subroutines (spec §5.1).

use crate::{handler_name, EventDispatcher, EventRegistry};
use oxbasic_runtime::pattern_matches;
use serde::{Deserialize, Serialize};

/// Classic BASIC control kinds (spec §5 mapping table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlKind {
    CommandButton,
    Label,
    TextBox,
    Frame,
    ListBox,
    ComboBox,
    PictureBox,
    Timer,
    CheckBox,
    OptionButton,
    /// VB's Image: a picture that can stretch to its size (no focus, no
    /// children).
    Image,
    /// VB's HScrollBar / VScrollBar: a horizontal or vertical slider over
    /// `Min`..`Max`, read and set through `Value`.
    HScrollBar,
    VScrollBar,
    /// VB's Line: a straight line from the control's top-left to its
    /// bottom-right (or, when `diagonal` is set, bottom-left to top-right).
    /// Its colour is `ForeColor`.
    Line,
    /// VB's Shape: a rectangle, square, oval or circle (`shape_type`),
    /// outlined in `ForeColor` and filled when `fill_style` is 0 (solid).
    Shape,
    /// A macOS volume list: a ListBox whose items are the mounted volumes
    /// (what `/Volumes` lists), read when the form loads. Used in place of
    /// VB's DriveListBox.
    VolumeListBox,
    /// VB's DirListBox: a list of the folders in `Path` (its sub-folders),
    /// read when the form loads and whenever `Path` changes.
    DirListBox,
    /// VB's FileListBox: the files in `Path` whose names match `Pattern`
    /// (`*.*` by default), read when the form loads and when either changes.
    FileListBox,
    /// VB's Slider (TrackBar): a draggable thumb over `Min`..`Max`, read and
    /// set through `Value`, with `Orientation` (0 horizontal, 1 vertical).
    Slider,
    /// VB's ProgressBar: a bar filled to `Value` over `Min`..`Max`. The
    /// program sets it; the user can't drag it.
    ProgressBar,
}

impl ControlKind {
    /// The BASIC type name as it appears in code / `.oxform` files.
    pub fn basic_name(&self) -> &'static str {
        match self {
            ControlKind::CommandButton => "CommandButton",
            ControlKind::Label => "Label",
            ControlKind::TextBox => "TextBox",
            ControlKind::Frame => "Frame",
            ControlKind::ListBox => "ListBox",
            ControlKind::ComboBox => "ComboBox",
            ControlKind::PictureBox => "PictureBox",
            ControlKind::Timer => "Timer",
            ControlKind::CheckBox => "CheckBox",
            ControlKind::OptionButton => "OptionButton",
            ControlKind::Image => "Image",
            ControlKind::HScrollBar => "HScrollBar",
            ControlKind::VScrollBar => "VScrollBar",
            ControlKind::Line => "Line",
            ControlKind::Shape => "Shape",
            ControlKind::VolumeListBox => "VolumeListBox",
            ControlKind::DirListBox => "DirListBox",
            ControlKind::FileListBox => "FileListBox",
            ControlKind::Slider => "Slider",
            ControlKind::ProgressBar => "ProgressBar",
        }
    }

    /// Parse a control kind from its BASIC name (case-insensitive).
    pub fn from_basic_name(name: &str) -> Option<ControlKind> {
        Some(match name.to_ascii_lowercase().as_str() {
            "commandbutton" | "button" => ControlKind::CommandButton,
            "label" => ControlKind::Label,
            "textbox" | "text" => ControlKind::TextBox,
            "frame" => ControlKind::Frame,
            "listbox" | "list" => ControlKind::ListBox,
            "combobox" | "combo" => ControlKind::ComboBox,
            "picturebox" | "picture" => ControlKind::PictureBox,
            "image" => ControlKind::Image,
            "hscrollbar" => ControlKind::HScrollBar,
            "vscrollbar" => ControlKind::VScrollBar,
            "line" => ControlKind::Line,
            "shape" => ControlKind::Shape,
            "volumelistbox" | "volumelist" => ControlKind::VolumeListBox,
            "dirlistbox" | "dirlist" => ControlKind::DirListBox,
            "filelistbox" | "filelist" => ControlKind::FileListBox,
            "slider" | "trackbar" => ControlKind::Slider,
            "progressbar" | "progress" => ControlKind::ProgressBar,
            "timer" => ControlKind::Timer,
            "checkbox" | "check" => ControlKind::CheckBox,
            "optionbutton" | "option" | "radio" => ControlKind::OptionButton,
            _ => return None,
        })
    }
}

/// A control's position and size in VB form coordinates (pixels here;
/// twips conversion arrives with the visual designer in Phase 3).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn new(left: f32, top: f32, width: f32, height: f32) -> Rect {
        Rect {
            left,
            top,
            width,
            height,
        }
    }
}

/// One classic BASIC control on a form.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlSpec {
    /// BASIC name, e.g. `Command1` — event subs derive from it
    /// (`Command1_Click`).
    pub name: String,
    pub kind: ControlKind,
    pub rect: Rect,
    /// `Caption` for buttons/labels/frames; window text for TextBox.
    #[serde(default)]
    pub caption: String,
    /// `Text` property (TextBox). Defaults to `caption` when absent.
    #[serde(default)]
    pub text: Option<String>,
    /// Items for ListBox / ComboBox.
    #[serde(default)]
    pub items: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub visible: bool,
    /// Timer interval in milliseconds (Timer only).
    #[serde(default)]
    pub interval_ms: u64,
    /// `Min`, `Max`, `SmallChange` and `LargeChange` of a scrollbar. VB's
    /// defaults: 0, 32767, 1 and 1.
    #[serde(default)]
    pub scroll_min: i32,
    #[serde(default = "default_scroll_max")]
    pub scroll_max: i32,
    #[serde(default = "default_one")]
    pub small_change: i32,
    #[serde(default = "default_one")]
    pub large_change: i32,
    /// `Shape` (Shape control): 0 rectangle, 1 square, 2 oval, 3 circle.
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub shape_type: u8,
    /// `Orientation` (Slider): 0 horizontal (the default), 1 vertical.
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub orientation: u8,
    /// `FillStyle` (Shape control): 0 solid, 1 transparent (VB's default).
    #[serde(default = "default_one_u8")]
    pub fill_style: u8,
    /// A Line control drawn from bottom-left to top-right instead.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub diagonal: bool,
    /// `Path` of a DirListBox / FileListBox: the folder whose entries it
    /// lists. Empty means the first mounted volume.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
    /// `Pattern` of a FileListBox: which file names it lists (`*.bas;*.oxproj`).
    /// Empty lists every file (`*.*`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pattern: String,
    /// `BackColor`, as 0xRRGGBB (`None`: the control's usual background).
    /// Named `color` in `.oxform` files, from when it was PictureBox-only.
    #[serde(default)]
    pub color: Option<u32>,
    /// `Style` (ComboBox): 0 = Dropdown Combo (editable text + drop-down
    /// list), 1 = Simple Combo (editable text + always-visible list),
    /// 2 = Dropdown List (pick only). Other kinds ignore it.
    #[serde(default)]
    pub style: u8,
    /// `Index` in a control array: controls sharing a `name` each have a
    /// distinct index, and their events receive it (`Index As Integer`).
    /// `None` for a control that is not in an array.
    #[serde(default)]
    pub index: Option<i32>,
    /// `Value`: a CheckBox's 0 unchecked / 1 checked / 2 grayed; an
    /// OptionButton's 0 / 1 (`False` / `True`).
    #[serde(default)]
    pub value: i32,
    /// `ForeColor`, as 0xRRGGBB (`None`: the usual text colour).
    #[serde(default)]
    pub fore_color: Option<u32>,
    /// The VB system colour (`&H800000nn`) `BackColor` / `ForeColor` were
    /// set to, if they were: the program reads that back, while `color` /
    /// `fore_color` hold what it looks like.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub back_system: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fore_system: Option<u32>,
    /// `Font`.
    #[serde(default)]
    pub font: FontSpec,
    /// `TabIndex`: where the control is in the form's tab order. `None`
    /// until the order is first set: the order controls were added in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_index: Option<i32>,
    /// `TabStop`: Tab stops on the control (a click still focuses it).
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub tab_stop: bool,
    /// `ToolTipText`: shown when the pointer rests on the control.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tool_tip: String,
    /// `MousePointer`: VB's pointer number (0 default, 1 arrow, 2 cross,
    /// 3 I-beam, 11 hourglass...).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub mouse_pointer: i16,
    /// `MouseIcon`: the image file shown as the pointer over the control
    /// when `MousePointer` is 99 (`vbCustom`), as given to `LoadPicture`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub mouse_icon: String,
    /// `Locked` (TextBox, ComboBox): the text can be read and selected but
    /// not typed into.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub locked: bool,
    /// `MaxLength` (TextBox): the most characters typing can make; 0 is no
    /// limit.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub max_length: u32,
    /// `MultiLine` (TextBox): Enter starts a new line and the text wraps.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub multi_line: bool,
    /// `ScrollBars` (TextBox): 0 none, 1 horizontal, 2 vertical, 3 both.
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub scroll_bars: u8,
    /// `MultiSelect` (ListBox): 0 one row, 1 a click toggles rows, 2 Shift
    /// and Ctrl extend the selection.
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub multi_select: u8,
    /// `Container`: the Frame or PictureBox (by name and index) the
    /// control sits in, its `Left`/`Top` relative to it; `None` is the form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    /// `Stretch` (Image): the picture is scaled to the control's size; off,
    /// the control sizes itself to the picture.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stretch: bool,
    /// `AutoSize` (PictureBox): the control takes the picture's size.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub auto_size: bool,
    /// `Picture` (PictureBox, Image): the image file it shows, as given to
    /// `LoadPicture` (relative to the project folder).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub picture: String,
}

fn is_zero(n: &i16) -> bool {
    *n == 0
}

fn is_zero_u8(n: &u8) -> bool {
    *n == 0
}

fn is_zero_u32(n: &u32) -> bool {
    *n == 0
}

fn is_true(b: &bool) -> bool {
    *b
}

/// A control's `Font`: `Name` ("" = the system font), `Size` in points,
/// and the style flags.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FontSpec {
    pub name: String,
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
}

/// The size a font has unless set: 10.5 pt, drawn 14 px high, the size the
/// controls have always been drawn at.
pub const DEFAULT_FONT_SIZE: f32 = 10.5;

/// The form's background colour, `0xRRGGBB`.
pub const FORM_BACK: u32 = 0xF1F0F5;

impl Default for FontSpec {
    fn default() -> FontSpec {
        FontSpec {
            name: String::new(),
            size: DEFAULT_FONT_SIZE,
            bold: false,
            italic: false,
            underline: false,
            strikethrough: false,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_scroll_max() -> i32 {
    32767
}

fn default_one() -> i32 {
    1
}

fn default_one_u8() -> u8 {
    1
}

impl ControlKind {
    /// Size in pixels a control gets when dropped from the toolbox, close
    /// to VB6's defaults (a Timer has no size: it is invisible at run time).
    pub fn default_size(&self) -> (f32, f32) {
        match self {
            ControlKind::CommandButton => (96.0, 28.0),
            ControlKind::Label => (100.0, 20.0),
            ControlKind::TextBox => (120.0, 24.0),
            ControlKind::Frame => (160.0, 100.0),
            ControlKind::ListBox => (120.0, 84.0),
            ControlKind::ComboBox => (120.0, 24.0),
            ControlKind::PictureBox => (100.0, 100.0),
            ControlKind::Timer => (0.0, 0.0),
            ControlKind::CheckBox | ControlKind::OptionButton => (110.0, 22.0),
            ControlKind::Image => (100.0, 100.0),
            ControlKind::HScrollBar => (120.0, 17.0),
            ControlKind::VScrollBar => (17.0, 120.0),
            ControlKind::Line => (120.0, 60.0),
            ControlKind::Shape => (100.0, 60.0),
            ControlKind::VolumeListBox => (120.0, 84.0),
            ControlKind::DirListBox => (120.0, 84.0),
            ControlKind::FileListBox => (120.0, 84.0),
            ControlKind::Slider => (160.0, 40.0),
            ControlKind::ProgressBar => (200.0, 20.0),
        }
    }

    /// Whether the control has a `Caption` property in VB6.
    pub fn has_caption(&self) -> bool {
        matches!(
            self,
            ControlKind::CommandButton
                | ControlKind::Label
                | ControlKind::Frame
                | ControlKind::CheckBox
                | ControlKind::OptionButton
        )
    }

    /// Whether the control has an editable `Text` property in VB6.
    pub fn has_text(&self) -> bool {
        matches!(self, ControlKind::TextBox | ControlKind::ComboBox)
    }

    /// Whether the control has a `Value` property (CheckBox, OptionButton).
    pub fn has_value(&self) -> bool {
        matches!(self, ControlKind::CheckBox | ControlKind::OptionButton)
    }

    /// Whether the control shows text, so `Font` and `ForeColor` apply.
    pub fn has_font(&self) -> bool {
        !matches!(
            self,
            ControlKind::PictureBox | ControlKind::Timer | ControlKind::Image
        )
    }

    /// Whether the user can drag the control's thumb (scrollbars and sliders;
    /// a progress bar only shows the program's value).
    pub fn is_draggable(&self) -> bool {
        matches!(
            self,
            ControlKind::HScrollBar | ControlKind::VScrollBar | ControlKind::Slider
        )
    }

    /// Whether the control is a scrollbar (`Min`, `Max`, `Value`...).
    pub fn is_scroll(&self) -> bool {
        matches!(
            self,
            ControlKind::HScrollBar
                | ControlKind::VScrollBar
                | ControlKind::Slider
                | ControlKind::ProgressBar
        )
    }

    /// Whether the control shows a `Picture`.
    pub fn has_picture(&self) -> bool {
        matches!(self, ControlKind::PictureBox | ControlKind::Image)
    }

    /// Whether other controls can sit inside it (`Container`).
    pub fn is_container(&self) -> bool {
        matches!(self, ControlKind::Frame | ControlKind::PictureBox)
    }

    /// Whether the control can take the keyboard focus (`SetFocus`, Tab).
    pub fn can_focus(&self) -> bool {
        matches!(
            self,
            ControlKind::CommandButton
                | ControlKind::TextBox
                | ControlKind::ListBox
                | ControlKind::ComboBox
                | ControlKind::CheckBox
                | ControlKind::OptionButton
                | ControlKind::PictureBox
        )
    }
}
/// The names of the mounted volumes, as `/Volumes` lists them, in order.
/// Hidden entries are left out; a `/Volumes` that can't be read gives none.
pub fn volume_names() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir("/Volumes")
        .map(|dir| {
            dir.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| !n.starts_with('.'))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// The sub-folders of `path`, sorted, hidden ones left out. A folder that
/// can't be read has none.
pub fn folder_names(path: &str) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(path)
        .map(|dir| {
            dir.filter_map(|e| e.ok())
                .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| !n.starts_with('.'))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// What a directory or file list shows: the folders of `path` for a
/// DirListBox, the files matching `pattern` for a FileListBox. Other kinds
/// show nothing.
pub fn list_items(kind: ControlKind, path: &str, pattern: &str) -> Vec<String> {
    match kind {
        ControlKind::DirListBox => folder_names(path),
        ControlKind::FileListBox => file_names(path, pattern),
        _ => Vec::new(),
    }
}


/// The regular files in `path` whose names match `pattern` (see
/// [`pattern_matches`]), sorted, hidden ones left out. A folder that can't be
/// read has none.
pub fn file_names(path: &str, pattern: &str) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(path)
        .map(|dir| {
            dir.filter_map(|e| e.ok())
                .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| !n.starts_with('.') && pattern_matches(n, pattern))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}



impl ControlSpec {
    /// place: enabled, visible, no caption or text, no items.
    pub fn new(name: &str, kind: ControlKind, rect: Rect) -> ControlSpec {
        ControlSpec {
            name: name.to_string(),
            kind,
            rect,
            caption: String::new(),
            text: None,
            items: vec![],
            enabled: true,
            visible: true,
            interval_ms: 0,
            scroll_min: 0,
            scroll_max: 32767,
            small_change: 1,
            large_change: 1,
            shape_type: 0,
            orientation: 0,
            fill_style: 1,
            diagonal: false,
            path: String::new(),
            pattern: String::new(),
            color: None,
            style: 0,
            index: None,
            value: 0,
            fore_color: None,
            back_system: None,
            fore_system: None,
            font: FontSpec::default(),
            tab_index: None,
            tab_stop: true,
            tool_tip: String::new(),
            mouse_pointer: 0,
            locked: false,
            mouse_icon: String::new(),
            max_length: 0,
            multi_line: false,
            scroll_bars: 0,
            multi_select: 0,
            container: None,
            picture: String::new(),
            stretch: false,
            auto_size: false,
        }
    }
}

impl ControlSpec {
    /// Effective text content (`Text` or `Caption`).
    pub fn text(&self) -> &str {
        self.text.as_deref().unwrap_or(&self.caption)
    }

    /// The conventional BASIC handler sub name for one of this control's
    /// events, e.g. `Command1` + `Click` → `Command1_Click`.
    pub fn handler(&self, event: &str) -> String {
        handler_name(&self.name, event)
    }
}

/// A classic VB form: top-level window plus its controls.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormSpec {
    pub name: String,
    pub caption: String,
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub controls: Vec<ControlSpec>,
    /// `KeyPreview`: the form's `KeyDown`, `KeyPress` and `KeyUp` run
    /// before the focused control's (and may cancel the key for it).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub key_preview: bool,
}

impl FormSpec {
    pub fn new(name: &str, caption: &str, width: f32, height: f32) -> FormSpec {
        FormSpec {
            name: name.to_string(),
            caption: caption.to_string(),
            width,
            height,
            controls: Vec::new(),
            key_preview: false,
        }
    }

    /// Builder: add a control, returning `&mut Self` for chaining.
    pub fn control(&mut self, spec: ControlSpec) -> &mut Self {
        self.controls.push(spec);
        self
    }

    /// Builder shortcut for a command button.
    pub fn button(&mut self, name: &str, caption: &str, rect: Rect) -> &mut Self {
        let mut c = ControlSpec::new(name, ControlKind::CommandButton, rect);
        c.caption = caption.to_string();
        self.control(c)
    }

    /// Builder shortcut for a label.
    pub fn label(&mut self, name: &str, caption: &str, rect: Rect) -> &mut Self {
        let mut c = ControlSpec::new(name, ControlKind::Label, rect);
        c.caption = caption.to_string();
        self.control(c)
    }

    /// Builder shortcut for a text box.
    pub fn text_box(&mut self, name: &str, text: &str, rect: Rect) -> &mut Self {
        let mut c = ControlSpec::new(name, ControlKind::TextBox, rect);
        c.text = Some(text.to_string());
        self.control(c)
    }

    /// Builder shortcut for a timer.
    pub fn timer(&mut self, name: &str, interval_ms: u64) -> &mut Self {
        let mut c = ControlSpec::new(name, ControlKind::Timer, Rect::new(0.0, 0.0, 0.0, 0.0));
        c.visible = false;
        c.interval_ms = interval_ms;
        self.control(c)
    }

    /// Look up a control by BASIC name (case-insensitive).
    pub fn control_by_name(&self, name: &str) -> Option<&ControlSpec> {
        self.controls
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name))
    }

    /// Load a `.oxform` JSON layout specification (spec §6).
    pub fn from_oxform(json: &str) -> Result<FormSpec, String> {
        serde_json::from_str(json).map_err(|e| format!("invalid .oxform: {}", e))
    }

    /// The controls in tab order (VB's `TabIndex`): by their `tab_index`,
    /// the ones never numbered after the numbered, in the order added.
    pub fn tab_order(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.controls.len()).collect();
        order.sort_by_key(|&i| (self.controls[i].tab_index.unwrap_or(i32::MAX), i));
        order
    }

    /// Control `i`'s `TabIndex`: its place in the tab order. Every control
    /// has one (VB numbers labels and frames too).
    pub fn tab_index_of(&self, i: usize) -> i32 {
        self.tab_order()
            .iter()
            .position(|&c| c == i)
            .map_or(0, |p| p as i32)
    }

    /// Set control `i`'s `TabIndex`, as VB does: it takes that place and
    /// the controls from there on move down one. Out-of-range numbers
    /// clamp to the ends. The whole order is then numbered explicitly.
    pub fn set_tab_index(&mut self, i: usize, n: i32) {
        let mut order = self.tab_order();
        order.retain(|&c| c != i);
        let at = (n.max(0) as usize).min(order.len());
        order.insert(at, i);
        for (place, &c) in order.iter().enumerate() {
            self.controls[c].tab_index = Some(place as i32);
        }
    }

    /// The controls Tab stops on, in tab order: they take the focus, are
    /// enabled and visible, and have `TabStop`.
    pub fn tab_stops(&self) -> Vec<usize> {
        self.tab_order()
            .into_iter()
            .filter(|&i| {
                let c = &self.controls[i];
                c.kind.can_focus() && c.enabled && c.visible && c.tab_stop && self.shown(i)
            })
            .collect()
    }

    /// The control that contains control `i` (its `Container`), if it is in
    /// one that exists.
    pub fn container_of(&self, i: usize) -> Option<usize> {
        let key = self.controls.get(i)?.container.as_deref()?;
        self.controls.iter().position(|c| {
            c.kind.is_container() && crate::container_key(c).eq_ignore_ascii_case(key)
        })
    }

    /// Whether `inner` sits (at any depth) inside container `outer`.
    pub fn contains(&self, outer: usize, inner: usize) -> bool {
        let mut at = self.container_of(inner);
        let mut steps = 0;
        while let Some(c) = at {
            if c == outer {
                return true;
            }
            steps += 1;
            if steps > self.controls.len() {
                return false; // a loop in a hand-edited file
            }
            at = self.container_of(c);
        }
        false
    }

    /// Control `i`'s position on the form: its `Left`/`Top` are from its
    /// container's inside, which is offset by the container's own position
    /// (a Frame's inside starts below its caption, as in VB: 0,0 is the
    /// frame's corner).
    pub fn form_origin(&self, i: usize) -> (f32, f32) {
        let r = self.controls[i].rect;
        let mut x = r.left;
        let mut y = r.top;
        let mut at = self.container_of(i);
        let mut steps = 0;
        while let Some(c) = at {
            x += self.controls[c].rect.left;
            y += self.controls[c].rect.top;
            steps += 1;
            if steps > self.controls.len() {
                break;
            }
            at = self.container_of(c);
        }
        (x, y)
    }

    /// Whether control `i` is visible on the form: it and every container
    /// around it is `Visible` (a hidden frame hides what is in it).
    pub fn shown(&self, i: usize) -> bool {
        let mut at = Some(i);
        let mut steps = 0;
        while let Some(c) = at {
            if !self.controls[c].visible {
                return false;
            }
            steps += 1;
            if steps > self.controls.len() {
                break;
            }
            at = self.container_of(c);
        }
        true
    }

    /// Whether control `i` takes input: it and its containers are enabled
    /// (a disabled frame disables what is in it, as in VB).
    pub fn usable(&self, i: usize) -> bool {
        let mut at = Some(i);
        let mut steps = 0;
        while let Some(c) = at {
            if !self.controls[c].enabled {
                return false;
            }
            steps += 1;
            if steps > self.controls.len() {
                break;
            }
            at = self.container_of(c);
        }
        true
    }

    /// Serialize to `.oxform` JSON (pretty).
    pub fn to_oxform(&self) -> String {
        serde_json::to_string_pretty(self).expect("serialize form")
    }
}

/// The event dispatch engine connecting UI events to BASIC subroutines.
///
/// `on_sub` receives the fully-derived handler name (e.g.
/// `Command1_Click`) and performs the call into the BASIC runtime
/// (spec §5.1: `runtime.evaluate_sub(...)`). The form's own events
/// (`Form_Load`, `Form_Unload`) use the form's name.
pub struct FormDispatcher {
    form: FormSpec,
    registry: EventRegistry,
}

impl FormDispatcher {
    pub fn new(form: FormSpec) -> FormDispatcher {
        FormDispatcher {
            form,
            registry: EventRegistry::default(),
        }
    }

    pub fn form(&self) -> &FormSpec {
        &self.form
    }

    /// Wire every control event that has a conventional handler. The
    /// caller supplies the list of sub names the BASIC program actually
    /// defines, so no dead registrations are created.
    pub fn wire_defined_subs<'a, I>(&mut self, defined_subs: I)
    where
        I: IntoIterator,
        I::Item: AsRef<str> + 'a,
    {
        let defined: Vec<String> = defined_subs
            .into_iter()
            .map(|s| s.as_ref().to_lowercase())
            .collect();
        let mut wire = |control: &str, event: &str| {
            let h = handler_name(control, event);
            if defined.contains(&h.to_lowercase()) {
                self.registry.register(control, event, &h);
            }
        };
        for c in &self.form.controls {
            for ev in crate::events_for_control(c.kind.basic_name()) {
                wire(&c.name, ev);
            }
        }
        // Form-level events: classic VB uses `Form_Load` regardless of
        // the form's own name, so wire both spellings.
        for ev in crate::events_for_control("Form") {
            wire("Form", ev);
            wire(&self.form.name, ev);
        }
    }

    /// Dispatch a UI event to BASIC: `dispatch("Command1", "Click", &mut rt)`.
    pub fn dispatch<F>(
        &self,
        control: &str,
        event: &str,
        evaluate_sub: &mut F,
    ) -> Result<(), String>
    where
        F: FnMut(&str) -> Result<(), String>,
    {
        for h in &self.registry.handlers(control, event) {
            evaluate_sub(h).map_err(|e| format!("{} failed: {}", h, e))?;
        }
        Ok(())
    }

    /// Which handlers are wired for `(control, event)` — for tests/tools.
    pub fn handlers_for(&self, control: &str, event: &str) -> Vec<String> {
        self.registry.handlers(control, event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oxform_round_trip() {
        let mut form = FormSpec::new("Form1", "Click me", 320.0, 200.0);
        form.button("Command1", "OK", Rect::new(16.0, 150.0, 88.0, 28.0));
        form.label("Label1", "Hello", Rect::new(16.0, 16.0, 200.0, 20.0));
        form.text_box("Text1", "type here", Rect::new(16.0, 48.0, 200.0, 24.0));
        form.timer("Timer1", 250);

        let json = form.to_oxform();
        let back = FormSpec::from_oxform(&json).unwrap();
        assert_eq!(back.name, "Form1");
        assert_eq!(back.controls.len(), 4);
        assert_eq!(
            back.control_by_name("command1").unwrap().kind,
            ControlKind::CommandButton
        );
        assert_eq!(back.control_by_name("text1").unwrap().text(), "type here");
        assert_eq!(back.control_by_name("timer1").unwrap().interval_ms, 250);
        assert_eq!(back.controls[0].rect.left, 16.0);
    }

    #[test]
    fn control_kind_parsing() {
        assert_eq!(
            ControlKind::from_basic_name("CommandButton"),
            Some(ControlKind::CommandButton)
        );
        assert_eq!(
            ControlKind::from_basic_name("textbox"),
            Some(ControlKind::TextBox)
        );
        assert_eq!(ControlKind::from_basic_name("nope"), None);
        assert_eq!(ControlKind::Timer.basic_name(), "Timer");
    }

    #[test]
    fn dispatch_routes_to_basic_subs() {
        let mut form = FormSpec::new("Form1", "Demo", 300.0, 200.0);
        form.button("Command1", "OK", Rect::new(10.0, 10.0, 80.0, 24.0));
        let mut d = FormDispatcher::new(form);
        d.wire_defined_subs(["Command1_Click", "Form_Load", "Command1_MouseDown"]);

        let called = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let mut eval = {
            let called = called.clone();
            move |sub: &str| -> Result<(), String> {
                called.borrow_mut().push(sub.to_string());
                Ok(())
            }
        };
        d.dispatch("Command1", "Click", &mut eval).unwrap();
        assert_eq!(*called.borrow(), vec!["Command1_Click"]);

        called.borrow_mut().clear();
        d.dispatch("Command1", "MouseDown", &mut eval).unwrap();
        assert_eq!(*called.borrow(), vec!["Command1_MouseDown"]);
        called.borrow_mut().clear();
        d.dispatch("Command1", "Change", &mut eval).unwrap();
        assert!(called.borrow().is_empty());

        // form-level events: classic `Form_Load` spelling
        d.dispatch("Form", "Load", &mut eval).unwrap();
        assert_eq!(*called.borrow(), vec!["Form_Load"]);

        let mut d2 = FormDispatcher::new(FormSpec::new("Form1", "x", 10.0, 10.0));
        d2.wire_defined_subs(["Command9_Click"]);
        assert!(d2.handlers_for("Form1", "Load").is_empty());
    }

    #[test]
    fn missing_event_handlers_are_noops_and_defined_handler_errors_propagate() {
        let mut form = FormSpec::new("Form1", "Demo", 300.0, 200.0);
        form.text_box("Text1", "", Rect::new(10.0, 10.0, 100.0, 24.0));
        form.button("Command1", "OK", Rect::new(10.0, 40.0, 80.0, 24.0));
        let mut dispatcher = FormDispatcher::new(form);
        dispatcher.wire_defined_subs(["Command1_Click"]);
        let mut calls = Vec::new();
        let mut evaluate = |name: &str| -> Result<(), String> {
            calls.push(name.to_string());
            Err("handler failed".to_string())
        };

        dispatcher
            .dispatch("Text1", "Click", &mut evaluate)
            .unwrap();
        dispatcher.dispatch("Form", "Load", &mut evaluate).unwrap();
        assert_eq!(
            dispatcher.dispatch("command1", "click", &mut evaluate),
            Err("Command1_Click failed: handler failed".to_string())
        );
        assert_eq!(calls, vec!["Command1_Click"]);
    }

    /// End-to-end with the real interpreter: a click runs BASIC code.
    #[test]
    fn dispatch_runs_basic() {
        use oxbasic_runtime::{Interpreter, Output};

        struct Sink(String);
        impl Output for Sink {
            fn write(&mut self, s: &str) {
                self.0.push_str(s);
            }
        }

        let src = "Sub Command1_Click()\n  MsgBox \"clicked!\"\nEnd Sub\n\
                   Function Add(a As Long, b As Long) As Long\n  Add = a + b\nEnd Function\n";
        let prog = oxbasic_parser::parse(src).unwrap();
        let sink = std::rc::Rc::new(std::cell::RefCell::new(String::new()));

        struct SharedSink(std::rc::Rc<std::cell::RefCell<String>>);
        impl Output for SharedSink {
            fn write(&mut self, s: &str) {
                self.0.borrow_mut().push_str(s);
            }
        }

        let mut shared = SharedSink(sink.clone());
        let mut rt = Interpreter::new(&prog, &mut shared);

        rt.evaluate_sub("Command1_Click", &[]).unwrap();
        assert_eq!(*sink.borrow(), "clicked!\n");
        let v = rt
            .evaluate_sub(
                "Add",
                &[
                    oxbasic_runtime::Variant::Long(3),
                    oxbasic_runtime::Variant::Long(4),
                ],
            )
            .unwrap();
        assert!(matches!(v, oxbasic_runtime::Variant::Long(7)));

        assert!(rt.evaluate_sub("Nope", &[]).is_err());
    }

    #[cfg(test)]
    mod key_preview_tests {
        use super::*;

        #[test]
        fn tab_order_follows_tab_index_and_defaults_to_the_order_added() {
            let mut f = FormSpec::new("Form1", "F", 100.0, 100.0);
            let r = Rect::new(0.0, 0.0, 10.0, 10.0);
            f.label("L", "", r);
            f.text_box("A", "", r);
            f.text_box("B", "", r);
            f.button("C", "", r);
            // never numbered: the order added; every control has a TabIndex
            assert_eq!(f.tab_order(), [0, 1, 2, 3]);
            assert_eq!(f.tab_index_of(2), 2);
            assert_eq!(f.tab_stops(), [1, 2, 3], "the label takes no focus");
            // C to the front: the others move down one, as in VB
            f.set_tab_index(3, 0);
            assert_eq!(f.tab_order(), [3, 0, 1, 2]);
            assert_eq!((f.tab_index_of(3), f.tab_index_of(0)), (0, 1));
            assert_eq!(f.tab_stops(), [3, 1, 2]);
            // out of range clamps to the end
            f.set_tab_index(3, 99);
            assert_eq!(f.tab_order(), [0, 1, 2, 3]);
            // TabStop off and disabled controls are skipped
            f.controls[1].tab_stop = false;
            f.controls[2].enabled = false;
            assert_eq!(f.tab_stops(), [3]);
            // the numbering is saved only once it was set
            let json = f.to_oxform();
            assert!(json.contains("tabIndex") && json.contains("tabStop"));
            assert!(!FormSpec::new("F", "F", 1.0, 1.0)
                .to_oxform()
                .contains("tabIndex"));
            let back = FormSpec::from_oxform(&json).unwrap();
            assert_eq!(back.tab_order(), f.tab_order());
            assert!(!back.controls[1].tab_stop);
        }

        #[test]
        fn key_preview_round_trips_and_older_files_default_to_off() {
            let mut f = FormSpec::new("Form1", "F", 100.0, 100.0);
            assert!(!f.to_oxform().contains("keyPreview"), "off isn't written");
            f.key_preview = true;
            let json = f.to_oxform();
            assert!(json.contains("\"keyPreview\": true"));
            assert!(FormSpec::from_oxform(&json).unwrap().key_preview);
            let old = r#"{"name":"Form1","caption":"F","width":10,"height":10}"#;
            assert!(!FormSpec::from_oxform(old).unwrap().key_preview);
        }
    }
}
