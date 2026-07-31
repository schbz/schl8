//! Register Schl8 as the default handler for the file types it opens.
//!
//! Finder's Get Info → "Open with:" → "Change All…" does this manually,
//! per type; this is the same thing through LaunchServices, so Help →
//! "Install & Default Editor…" can offer one click for all of them.
//!
//! Only file *associations* are touched — no system settings, no
//! privileged operations. Any of it is undone in Finder the same way it
//! would be for any other app.

use std::ffi::c_void;

use anyhow::{anyhow, Result};
use objc2_foundation::NSString;

/// Bundle identifier declared by `scripts/bundle.sh`.
const BUNDLE_ID: &str = "com.functiondesk.schl8";

/// `kLSRolesAll` — Schl8 is registered as an Editor for these types.
const LS_ROLES_ALL: u32 = 0xFFFF_FFFF;

#[link(name = "CoreServices", kind = "framework")]
extern "C" {
    fn LSSetDefaultRoleHandlerForContentType(
        in_content_type: *const c_void,
        in_role: u32,
        in_handler_bundle_id: *const c_void,
    ) -> i32;
}

/// The types Schl8 can be made default for: (UTI, human label).
/// The `com.functiondesk.schl8.*` UTIs are the ones the bundle
/// exports for OpenPGP files (macOS ships none).
pub const HANDLED_TYPES: &[(&str, &str)] = &[
    ("com.functiondesk.schl8.gpg", "Encrypted files (.gpg, .pgp)"),
    ("com.functiondesk.schl8.asc", "Armored files (.asc)"),
    ("net.daringfireball.markdown", "Markdown (.md)"),
    ("public.plain-text", "Plain text (.txt)"),
];

/// Make Schl8 the default application for `uti`.
///
/// Requires the app bundle to be installed (LaunchServices resolves the
/// bundle id); running from `cargo run` will fail with an OS error.
pub fn set_default_for(uti: &str) -> Result<()> {
    // NSString is toll-free bridged to CFStringRef.
    let uti_ns = NSString::from_str(uti);
    let bundle_ns = NSString::from_str(BUNDLE_ID);
    let status = unsafe {
        LSSetDefaultRoleHandlerForContentType(
            (&*uti_ns as *const NSString).cast(),
            LS_ROLES_ALL,
            (&*bundle_ns as *const NSString).cast(),
        )
    };
    if status == 0 {
        Ok(())
    } else {
        Err(anyhow!(
            "LaunchServices refused {uti} (status {status}) — is Schl8.app \
             installed in /Applications?"
        ))
    }
}

/// Make Schl8 the default for every type it handles. Returns the list
/// of failures (empty on full success) so the caller can report exactly
/// which types didn't take.
pub fn set_default_for_all() -> Vec<String> {
    HANDLED_TYPES
        .iter()
        .filter_map(|(uti, label)| set_default_for(uti).err().map(|e| format!("{label}: {e}")))
        .collect()
}
