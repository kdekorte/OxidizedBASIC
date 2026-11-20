//! The macOS "waiting" cursor for VB's `vbHourglass` (`MousePointer` 11,
//! and 13, arrow and hourglass).
//!
//! GPUI's `CursorStyle` has no wait cursor, and its macOS backend turns each
//! style into a call of one `NSCursor` class method. It never uses
//! `CursorStyle::DragCopy` for anything this app draws, so that style
//! stands in for the hourglass: `+[NSCursor dragCopyCursor]` is pointed at
//! `+[NSCursor busyButClickableCursor]`, the spinning pointer macOS shows
//! for a busy app. That one is a private method, so it is looked up first;
//! if this macOS doesn't have it nothing changes and the arrow is shown.

/// Make `CursorStyle::DragCopy` show the wait cursor. True when it does
/// (this macOS has the cursor); false anywhere else, where the caller uses
/// the arrow. Safe to call again.
#[cfg(target_os = "macos")]
pub fn install_wait_cursor() -> bool {
    use objc::runtime::{method_getImplementation, method_setImplementation, Class, Method, Sel};
    use std::sync::OnceLock;
    static INSTALLED: OnceLock<bool> = OnceLock::new();
    *INSTALLED.get_or_init(|| {
        let Some(cursor) = Class::get("NSCursor") else {
            return false;
        };
        let meta = cursor.metaclass();
        let busy = meta.instance_method(Sel::register("busyButClickableCursor"));
        let target = meta.instance_method(Sel::register("dragCopyCursor"));
        let (Some(busy), Some(target)) = (busy, target) else {
            return false;
        };
        // both are `+ (NSCursor *)name` with no arguments, so one can
        // stand in for the other
        unsafe {
            let imp = method_getImplementation(busy as *const Method);
            method_setImplementation(target as *const Method as *mut Method, imp);
        }
        true
    })
}

#[cfg(not(target_os = "macos"))]
pub fn install_wait_cursor() -> bool {
    false
}

/// Whether the image file `path` can be shown as a pointer (macOS, and an
/// image it reads), without making it the pointer.
pub fn icon_cursor_usable(path: &std::path::Path) -> bool {
    #[cfg(target_os = "macos")]
    {
        imp::usable(path)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        false
    }
}

/// Show the image file `path` as the pointer (VB's `MouseIcon`, with
/// `MousePointer` 99) over whatever sets `CursorStyle::DragLink` — the
/// window calls this as the pointer enters a control that has an icon, and
/// with `None` as it leaves. The image is scaled down to 32 by 32 pixels
/// when larger, and its top-left corner is the hot spot. Returns whether
/// the icon can be shown (macOS, and the file is an image it can read);
/// otherwise the caller shows the pointing hand.
#[cfg(target_os = "macos")]
pub fn set_icon_cursor(path: Option<&std::path::Path>) -> bool {
    imp::set_icon(path)
}

#[cfg(not(target_os = "macos"))]
pub fn set_icon_cursor(_path: Option<&std::path::Path>) -> bool {
    false
}

#[cfg(target_os = "macos")]
// objc 0.2's macros test a `cargo-clippy` cfg this crate doesn't declare
#[allow(unexpected_cfgs)]
mod imp {
    use objc::runtime::{
        method_getImplementation, method_setImplementation, Class, Imp, Method, Object, Sel,
    };
    use objc::{class, msg_send, sel, sel_impl};
    use std::collections::HashMap;
    use std::ffi::CString;
    use std::os::raw::c_char;
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, OnceLock};

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Point {
        x: f64,
        y: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Size {
        w: f64,
        h: f64,
    }
    unsafe impl objc::Encode for Point {
        fn encode() -> objc::Encoding {
            unsafe { objc::Encoding::from_str("{CGPoint=dd}") }
        }
    }
    unsafe impl objc::Encode for Size {
        fn encode() -> objc::Encoding {
            unsafe { objc::Encoding::from_str("{CGSize=dd}") }
        }
    }

    /// The cursor `dragLinkCursor` answers with now, as an address (a
    /// pointer isn't `Send`); `None` is the original cursor.
    static CURRENT: Mutex<Option<PathBuf>> = Mutex::new(None);
    /// A cursor made for each icon file, kept for good (there are few).
    static MADE: OnceLock<Mutex<HashMap<PathBuf, usize>>> = OnceLock::new();
    /// `dragLinkCursor` as it was.
    static ORIGINAL: OnceLock<usize> = OnceLock::new();

    /// The replacement for `+[NSCursor dragLinkCursor]`.
    extern "C" fn drag_link_cursor(this: &Object, _cmd: Sel) -> *mut Object {
        let path = CURRENT.lock().ok().and_then(|c| c.clone());
        if let Some(p) = path {
            if let Some(c) = cursor_for_file(&p) {
                return c as *mut Object;
            }
        }
        // the pointing hand stands in when there is no icon
        unsafe { msg_send![this, pointingHandCursor] }
    }

    fn cursor_for_file(path: &Path) -> Option<usize> {
        let made = MADE.get_or_init(Default::default);
        if let Some(&c) = made.lock().ok()?.get(path) {
            return Some(c);
        }
        let cpath = CString::new(path.to_str()?).ok()?;
        let cursor = unsafe {
            let ns: *mut Object =
                msg_send![class!(NSString), stringWithUTF8String: cpath.as_ptr() as *const c_char];
            let image: *mut Object = msg_send![class!(NSImage), alloc];
            let image: *mut Object = msg_send![image, initWithContentsOfFile: ns];
            if image.is_null() {
                return None;
            }
            // real cursors are 32x32; a big picture would cover the screen
            let size: Size = msg_send![image, size];
            if size.w > 32.0 || size.h > 32.0 {
                let k = (32.0 / size.w).min(32.0 / size.h);
                let _: () = msg_send![image, setSize: Size { w: size.w * k, h: size.h * k }];
            }
            let cursor: *mut Object = msg_send![class!(NSCursor), alloc];
            let cursor: *mut Object =
                msg_send![cursor, initWithImage: image hotSpot: Point { x: 0.0, y: 0.0 }];
            if cursor.is_null() {
                return None;
            }
            cursor as usize
        };
        made.lock().ok()?.insert(path.to_path_buf(), cursor);
        Some(cursor)
    }

    pub fn usable(path: &Path) -> bool {
        // the method is ours before any control asks for DragLink
        install();
        cursor_for_file(path).is_some()
    }

    fn install() {
        ORIGINAL.get_or_init(|| unsafe {
            let meta = Class::get("NSCursor").expect("NSCursor").metaclass();
            let m = meta
                .instance_method(Sel::register("dragLinkCursor"))
                .expect("dragLinkCursor") as *const Method as *mut Method;
            let original = method_getImplementation(m as *const Method) as usize;
            let ours: extern "C" fn(&Object, Sel) -> *mut Object = drag_link_cursor;
            method_setImplementation(m, std::mem::transmute::<_, Imp>(ours));
            original
        });
    }

    pub fn set_icon(path: Option<&Path>) -> bool {
        install();
        let usable = match path {
            Some(p) => cursor_for_file(p).is_some(),
            None => true,
        };
        if let Ok(mut c) = CURRENT.lock() {
            *c = path.filter(|_| usable).map(Path::to_path_buf);
        }
        usable
    }
}

#[cfg(all(test, target_os = "macos"))]
#[allow(unexpected_cfgs)]
mod tests {
    use super::*;
    use objc::runtime::Object;
    use objc::{msg_send, sel, sel_impl};

    #[link(name = "AppKit", kind = "framework")]
    extern "C" {}

    #[test]
    fn the_drag_copy_cursor_becomes_the_wait_cursor() {
        assert!(install_wait_cursor(), "this macOS has the busy cursor");
        assert!(install_wait_cursor(), "and asking again is harmless");
        let class = objc::runtime::Class::get("NSCursor").unwrap();
        let (a, b): (*mut Object, *mut Object) = unsafe {
            (
                msg_send![class, dragCopyCursor],
                msg_send![class, busyButClickableCursor],
            )
        };
        assert!(!a.is_null());
        assert_eq!(a, b, "dragCopyCursor now is busyButClickableCursor");
        // the others are untouched: the arrow is still the arrow
        let arrow: *mut Object = unsafe { msg_send![class, arrowCursor] };
        assert_ne!(arrow, a);
    }

    #[test]
    fn an_icon_becomes_the_drag_link_cursor_only_while_it_is_set() {
        let logo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/showcase/logo.png");
        assert!(icon_cursor_usable(&logo), "the PNG makes a cursor");
        assert!(!icon_cursor_usable(std::path::Path::new("/no/such.png")));
        assert!(
            !icon_cursor_usable(std::path::Path::new("/etc/hosts")),
            "not an image"
        );

        let get = |sel_name: &str| -> usize {
            let class = objc::runtime::Class::get("NSCursor").unwrap();
            let p: *mut Object = match sel_name {
                "dragLinkCursor" => unsafe { msg_send![class, dragLinkCursor] },
                _ => unsafe { msg_send![class, pointingHandCursor] },
            };
            p as usize
        };
        // nothing set: the pointing hand answers
        set_icon_cursor(None);
        assert_eq!(get("dragLinkCursor"), get("pointingHandCursor"));
        // set: a cursor of its own, the same one each time
        assert!(set_icon_cursor(Some(&logo)));
        let icon = get("dragLinkCursor");
        assert_ne!(icon, get("pointingHandCursor"));
        assert_eq!(icon, get("dragLinkCursor"), "made once");
        // a file that isn't an image can't be set, and the hand returns
        assert!(!set_icon_cursor(Some(std::path::Path::new("/etc/hosts"))));
        assert_eq!(get("dragLinkCursor"), get("pointingHandCursor"));
        // cleared again
        set_icon_cursor(Some(&logo));
        set_icon_cursor(None);
        assert_eq!(get("dragLinkCursor"), get("pointingHandCursor"));
    }
}
