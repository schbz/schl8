//! Lock-on-sleep: observe macOS power/lock notifications and raise a flag
//! the app polls each frame, so Schl8 can auto-lock (zeroize plaintext)
//! when the display sleeps, the system sleeps, or the screen is locked.
//!
//! `NSWorkspace`'s notification center delivers sleep/screensaver events;
//! `screenIsLocked` arrives via the distributed notification center. We
//! register a small observer object for each and set an atomic flag.

use std::sync::atomic::{AtomicBool, Ordering};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{
    class, declare_class, msg_send, msg_send_id, mutability, sel, ClassType, DeclaredClass,
};
use objc2_foundation::{NSObject, NSString};

static LOCK_REQUESTED: AtomicBool = AtomicBool::new(false);
static REPAINT_CTX: std::sync::Mutex<Option<egui::Context>> = std::sync::Mutex::new(None);

declare_class!(
    struct PowerObserver;

    unsafe impl ClassType for PowerObserver {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "Schl8PowerObserver";
    }

    impl DeclaredClass for PowerObserver {}

    unsafe impl PowerObserver {
        #[method(lockNow:)]
        fn lock_now(&self, _notification: *mut AnyObject) {
            // Contain any panic — this is called from Cocoa (FFI boundary).
            let _ = std::panic::catch_unwind(|| {
                LOCK_REQUESTED.store(true, Ordering::SeqCst);
                if let Ok(ctx) = REPAINT_CTX.lock() {
                    if let Some(ctx) = ctx.as_ref() {
                        ctx.request_repaint();
                    }
                }
            });
        }
    }
);

/// Register observers for sleep / screensaver / screen-lock. Safe to call
/// once at startup; failures are silently ignored (the idle timeout still
/// provides protection).
pub fn install(ctx: &egui::Context) {
    if let Ok(mut slot) = REPAINT_CTX.lock() {
        *slot = Some(ctx.clone());
    }
    unsafe {
        let observer: Retained<PowerObserver> = msg_send_id![PowerObserver::alloc(), init];
        let sel = sel!(lockNow:);

        // NSWorkspace notifications: system will sleep, screen did sleep.
        let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
        if !workspace.is_null() {
            let nc: *mut AnyObject = msg_send![workspace, notificationCenter];
            if !nc.is_null() {
                for name in [
                    "NSWorkspaceWillSleepNotification",
                    "NSWorkspaceScreensDidSleepNotification",
                ] {
                    let ns_name = NSString::from_str(name);
                    let _: () = msg_send![
                        nc,
                        addObserver: &*observer,
                        selector: sel,
                        name: &*ns_name,
                        object: std::ptr::null_mut::<AnyObject>(),
                    ];
                }
            }
        }

        // Distributed notifications: screen locked, screensaver started.
        let dnc: *mut AnyObject = msg_send![class!(NSDistributedNotificationCenter), defaultCenter];
        if !dnc.is_null() {
            for name in ["com.apple.screenIsLocked", "com.apple.screensaver.didstart"] {
                let ns_name = NSString::from_str(name);
                let _: () = msg_send![
                    dnc,
                    addObserver: &*observer,
                    selector: sel,
                    name: &*ns_name,
                    object: std::ptr::null_mut::<AnyObject>(),
                ];
            }
        }

        // The observer must live for the process lifetime; the notification
        // centers hold it weakly.
        std::mem::forget(observer);
    }
}

/// Take (and clear) a pending lock request raised by a power/lock event.
pub fn take_lock_request() -> bool {
    LOCK_REQUESTED.swap(false, Ordering::SeqCst)
}
