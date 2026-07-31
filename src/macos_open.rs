//! Finder / Dock integration: open documents from "Open With → Schl8",
//! double-clicks, drops on the Dock icon — and re-showing the window when
//! the Dock icon is clicked while Schl8 is resident with no visible
//! window (`applicationShouldHandleReopen:hasVisibleWindows:`).
//!
//! Because our bundle declares `CFBundleDocumentTypes`, AppKit routes
//! those opens through the *NSApplication delegate* (`application:openURLs:`,
//! or the legacy `openFile:`/`openFiles:`). winit owns the delegate and
//! implements none of these, so without them AppKit shows
//! "cannot open files in the …​ format".
//!
//! The catch is timing: AppKit decides whether the delegate can open
//! documents during launch, *before* eframe's creation callback runs — so
//! adding the methods there (or installing a raw `odoc` Apple-event
//! handler, which AppKit's `finishLaunching` unconditionally replaces) is
//! too late for the file that *cold-launched* the app.
//!
//! Fix: swizzle `-[NSApplication setDelegate:]`. winit calls it during
//! `EventLoop` construction — before `finishLaunching` — so when it
//! installs its delegate we inject the open-methods into that delegate's
//! class right then, in time for the launch open. `install_early()` sets
//! the swizzle up from `main()`, before eframe runs.

use std::ffi::c_void;
use std::os::raw::c_char;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

use objc2::runtime::AnyObject;
use objc2::{class, msg_send};
use objc2_foundation::NSString;

static OPEN_REQUESTS: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
/// Set when the user clicks the Dock icon with no window showing.
static REOPEN_REQUESTED: AtomicBool = AtomicBool::new(false);
static REPAINT_CTX: Mutex<Option<egui::Context>> = Mutex::new(None);
/// Original `-[NSApplication setDelegate:]` IMP, saved by the swizzle.
static ORIG_SET_DELEGATE: AtomicUsize = AtomicUsize::new(0);

fn queue_path(p: PathBuf) {
    if let Ok(mut q) = OPEN_REQUESTS.lock() {
        q.push(p);
    }
    if let Ok(ctx) = REPAINT_CTX.lock() {
        if let Some(ctx) = ctx.as_ref() {
            ctx.request_repaint();
        }
    }
}

// ── Delegate open-methods (added to winit's delegate class) ───────────

/// `-(void)application:(id)app openURLs:(NSArray<NSURL *> *)urls`
extern "C" fn application_open_urls(
    _this: *mut AnyObject,
    _cmd: *const c_void,
    _app: *mut AnyObject,
    urls: *mut AnyObject,
) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        if urls.is_null() {
            return;
        }
        let count: usize = msg_send![urls, count];
        for i in 0..count {
            let url: *mut AnyObject = msg_send![urls, objectAtIndex: i];
            if url.is_null() {
                continue;
            }
            // NSURL.path yields the decoded filesystem path directly.
            let path: *mut NSString = msg_send![url, path];
            if !path.is_null() {
                queue_path(PathBuf::from((*path).to_string()));
            }
        }
    }));
}

/// `-(BOOL)application:(id)app openFile:(NSString *)filename`
extern "C" fn application_open_file(
    _this: *mut AnyObject,
    _cmd: *const c_void,
    _app: *mut AnyObject,
    filename: *mut AnyObject,
) -> bool {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        if filename.is_null() {
            return false;
        }
        let s = filename as *mut NSString;
        queue_path(PathBuf::from((*s).to_string()));
        true
    }))
    .unwrap_or(false)
}

/// `-(void)application:(id)app openFiles:(NSArray<NSString *> *)files`
extern "C" fn application_open_files(
    _this: *mut AnyObject,
    _cmd: *const c_void,
    app: *mut AnyObject,
    files: *mut AnyObject,
) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        if !files.is_null() {
            let count: usize = msg_send![files, count];
            for i in 0..count {
                let s: *mut NSString = msg_send![files, objectAtIndex: i];
                if !s.is_null() {
                    queue_path(PathBuf::from((*s).to_string()));
                }
            }
        }
        // NSApplicationDelegateReplySuccess = 0
        if !app.is_null() {
            let _: () = msg_send![app, replyToOpenOrPrint: 0usize];
        }
    }));
}

/// `-(BOOL)applicationShouldHandleReopen:(id)app hasVisibleWindows:(BOOL)flag`
///
/// AppKit calls this when the Dock icon is clicked. While Schl8 is
/// resident in the menu bar its window is merely hidden (ordered out),
/// not closed — AppKit then sees no windows to un-minimize and does
/// nothing, so the click appeared dead. Flag it for the app loop, which
/// makes the window visible again.
extern "C" fn application_should_handle_reopen(
    _this: *mut AnyObject,
    _cmd: *const c_void,
    _app: *mut AnyObject,
    has_visible_windows: bool,
) -> bool {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if !has_visible_windows {
            REOPEN_REQUESTED.store(true, Ordering::SeqCst);
            if let Ok(ctx) = REPAINT_CTX.lock() {
                if let Some(ctx) = ctx.as_ref() {
                    ctx.request_repaint();
                }
            }
        }
    }));
    // YES: let AppKit also perform its default un-minimize handling.
    true
}

/// Add the open/reopen methods to `cls` if not already present.
unsafe fn inject_open_methods(cls: *mut objc2::ffi::objc_class) {
    let add = |sel: &[u8], imp: objc2::ffi::IMP, types: &[u8]| {
        let s = objc2::ffi::sel_registerName(sel.as_ptr() as *const c_char);
        objc2::ffi::class_addMethod(cls, s, imp, types.as_ptr() as *const c_char);
    };
    add(
        b"application:openURLs:\0",
        Some(std::mem::transmute::<
            extern "C" fn(*mut AnyObject, *const c_void, *mut AnyObject, *mut AnyObject),
            unsafe extern "C" fn(),
        >(application_open_urls)),
        b"v@:@@\0",
    );
    add(
        b"application:openFile:\0",
        Some(std::mem::transmute::<
            extern "C" fn(*mut AnyObject, *const c_void, *mut AnyObject, *mut AnyObject) -> bool,
            unsafe extern "C" fn(),
        >(application_open_file)),
        b"B@:@@\0",
    );
    add(
        b"application:openFiles:\0",
        Some(std::mem::transmute::<
            extern "C" fn(*mut AnyObject, *const c_void, *mut AnyObject, *mut AnyObject),
            unsafe extern "C" fn(),
        >(application_open_files)),
        b"v@:@@\0",
    );
    add(
        b"applicationShouldHandleReopen:hasVisibleWindows:\0",
        Some(std::mem::transmute::<
            extern "C" fn(*mut AnyObject, *const c_void, *mut AnyObject, bool) -> bool,
            unsafe extern "C" fn(),
        >(application_should_handle_reopen)),
        b"B@:@B\0",
    );
}

// ── setDelegate: swizzle ──────────────────────────────────────────────

/// Replacement for `-[NSApplication setDelegate:]`: inject the
/// open-methods into the incoming delegate's class, then call through to
/// the original implementation. Runs (via winit) before `finishLaunching`.
extern "C" fn swizzled_set_delegate(
    this: *mut AnyObject,
    _cmd: *const c_void,
    delegate: *mut AnyObject,
) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        if !delegate.is_null() {
            let cls = objc2::ffi::object_getClass(delegate.cast());
            inject_open_methods(cls as *mut _);
        }
    }));
    unsafe {
        let orig = ORIG_SET_DELEGATE.load(Ordering::SeqCst);
        if orig != 0 {
            let orig: extern "C" fn(*mut AnyObject, *const c_void, *mut AnyObject) =
                std::mem::transmute(orig);
            orig(this, _cmd, delegate);
        }
    }
}

/// Swizzle `-[NSApplication setDelegate:]`. Call once from `main()` before
/// the event loop is built. This is the only point early enough to have
/// the delegate's open-methods in place for a cold-launch Finder open.
pub fn install_early() {
    unsafe {
        let nsapp: *const objc2::ffi::objc_class =
            (class!(NSApplication) as *const objc2::runtime::AnyClass).cast();
        let sel = objc2::ffi::sel_registerName(b"setDelegate:\0".as_ptr() as *const c_char);
        let method = objc2::ffi::class_getInstanceMethod(nsapp, sel);
        if method.is_null() {
            eprintln!("schl8: could not find NSApplication setDelegate:");
            return;
        }
        let orig = objc2::ffi::method_getImplementation(method);
        ORIG_SET_DELEGATE.store(orig.map_or(0, |f| f as usize), Ordering::SeqCst);

        let new_imp: objc2::ffi::IMP = Some(std::mem::transmute::<
            extern "C" fn(*mut AnyObject, *const c_void, *mut AnyObject),
            unsafe extern "C" fn(),
        >(swizzled_set_delegate));
        objc2::ffi::method_setImplementation(method, new_imp);
    }
}

/// Record the egui context so queued opens trigger a repaint. Call from
/// eframe's creation callback. Also (belt-and-suspenders) injects the
/// open-methods for the already-running case.
pub fn install(ctx: &egui::Context) {
    if let Ok(mut slot) = REPAINT_CTX.lock() {
        *slot = Some(ctx.clone());
    }
    unsafe {
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        if app.is_null() {
            return;
        }
        let delegate: *mut AnyObject = msg_send![app, delegate];
        if !delegate.is_null() {
            let cls = objc2::ffi::object_getClass(delegate.cast());
            inject_open_methods(cls as *mut _);
        }
    }
}

/// Whether the Dock icon was clicked with no visible window since the
/// last check (consumes the flag).
pub fn take_reopen_request() -> bool {
    REOPEN_REQUESTED.swap(false, Ordering::SeqCst)
}

/// Files queued by Finder since the last frame.
pub fn drain_requests() -> Vec<PathBuf> {
    OPEN_REQUESTS
        .lock()
        .map(|mut q| q.drain(..).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    /// The injected selectors must actually register on a class — a typo
    /// in a selector name or type encoding would otherwise only show up
    /// as AppKit silently never calling us (the bug this module exists
    /// to prevent).
    #[test]
    fn injects_delegate_selectors() {
        unsafe {
            let name = CString::new("Schl8InjectTestDelegate").unwrap();
            let superclass: *const objc2::ffi::objc_class =
                (class!(NSObject) as *const objc2::runtime::AnyClass).cast();
            let cls = objc2::ffi::objc_allocateClassPair(superclass, name.as_ptr(), 0);
            assert!(!cls.is_null(), "could not create test class");
            inject_open_methods(cls);
            objc2::ffi::objc_registerClassPair(cls);

            for sel_name in [
                "application:openURLs:",
                "application:openFile:",
                "application:openFiles:",
                "applicationShouldHandleReopen:hasVisibleWindows:",
            ] {
                let c = CString::new(sel_name).unwrap();
                let sel = objc2::ffi::sel_registerName(c.as_ptr());
                let m = objc2::ffi::class_getInstanceMethod(cls.cast_const(), sel);
                assert!(!m.is_null(), "selector {sel_name} was not injected");
            }
        }
    }
}
