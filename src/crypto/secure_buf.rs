use zeroize::Zeroize;

use crate::security::memory;

/// A secure buffer that holds sensitive data in mlock'd memory and
/// zeroizes it on drop. Does not implement Clone, Debug, or Display
/// to prevent accidental exposure.
pub struct SecureBuffer {
    data: Vec<u8>,
    locked: bool,
}

#[allow(dead_code)]
impl SecureBuffer {
    /// Create a SecureBuffer from a byte vector, consuming and replacing
    /// the original. The buffer's memory is locked to prevent swapping.
    pub fn from_bytes(mut bytes: Vec<u8>) -> Self {
        // Allocate our own vec and copy the data
        let data = bytes.clone();

        // Zeroize the original allocation immediately
        bytes.zeroize();

        // Lock our buffer in memory
        let locked = if !data.is_empty() {
            unsafe { memory::mlock(data.as_ptr(), data.len()) }
        } else {
            true
        };

        if !locked {
            eprintln!("warning: could not mlock secure buffer — data may be swapped to disk");
        }

        SecureBuffer { data, locked }
    }

    /// Borrow the buffer contents as a UTF-8 string slice.
    pub fn as_str(&self) -> anyhow::Result<&str> {
        std::str::from_utf8(&self.data)
            .map_err(|e| anyhow::anyhow!("decrypted content is not valid UTF-8: {e}"))
    }

    /// Borrow the raw bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    /// Length in bytes.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

impl Drop for SecureBuffer {
    fn drop(&mut self) {
        // Capture the region before zeroize: Vec::zeroize clears the vec
        // (len becomes 0), so the pointer/length must be saved first.
        let ptr = self.data.as_ptr();
        let capacity = self.data.capacity();

        // Zeroize the data using volatile writes (wipes the full capacity)
        self.data.zeroize();

        // Unlock the memory region
        if self.locked && capacity > 0 {
            unsafe {
                memory::munlock(ptr, capacity);
            }
        }
    }
}

// ── Compile-time security properties ─────────────────────────────────
// These assertions turn the security-relevant trait (non-)implementations
// from happy accidents into compiler-enforced guarantees: if a future
// change adds a leaking impl (e.g. #[derive(Clone)]) or alters thread
// affinity, the build fails here instead of silently weakening the model.

// SecureBuffer must never be copyable, loggable, or printable — but it
// MUST stay Send: decrypted documents are produced on the background
// decrypt thread and handed to the UI over an mpsc channel.
static_assertions::assert_impl_all!(SecureBuffer: Send);
static_assertions::assert_not_impl_any!(
    SecureBuffer: Clone,
    std::fmt::Debug,
    std::fmt::Display
);

/// Extra capacity reserved up front for edit buffers so that typical edits
/// never force the String to reallocate (a reallocation would move plaintext
/// to a new address, leaving a stale copy in freed memory that we cannot
/// zeroize). 256 KiB of slack makes reallocation rare for text documents.
const EDIT_SLACK: usize = 256 * 1024;

/// A mutable String wrapper for editing sensitive text.
/// Zeroizes its contents on drop. Provides `&mut String` access
/// for egui's TextEdit widget while ensuring cleanup.
///
/// The full capacity of the String is mlock'd. Callers that mutate the
/// String (e.g. via `as_mut_string`) must call `relock_if_moved` afterwards
/// so the lock follows the allocation if it was moved by a reallocation.
pub struct SecureString {
    data: String,
    /// Start of the currently mlock'd region (null when nothing is locked).
    locked_ptr: *const u8,
    /// Length of the currently mlock'd region.
    locked_len: usize,
}

impl SecureString {
    /// Create an empty SecureString with slack capacity, mlock'd.
    pub fn empty() -> Self {
        let mut secure = SecureString {
            data: String::with_capacity(EDIT_SLACK),
            locked_ptr: std::ptr::null(),
            locked_len: 0,
        };
        secure.lock_current();
        secure
    }

    /// Create a SecureString by copying content from a SecureBuffer.
    /// The new allocation is mlock'd independently, with slack capacity
    /// reserved so edits rarely trigger a reallocation.
    pub fn from_secure_buffer(buf: &SecureBuffer) -> anyhow::Result<Self> {
        let src = buf.as_str()?;
        let mut s = String::with_capacity(src.len() + EDIT_SLACK);
        s.push_str(src);

        let mut secure = SecureString {
            data: s,
            locked_ptr: std::ptr::null(),
            locked_len: 0,
        };
        secure.lock_current();
        Ok(secure)
    }

    /// Borrow the inner string for reading.
    pub fn as_str(&self) -> &str {
        &self.data
    }

    /// Borrow the contents as bytes.
    pub fn as_bytes(&self) -> &[u8] {
        self.data.as_bytes()
    }

    /// Mutable access to the inner String (for egui TextEdit).
    /// Call `relock_if_moved` after the mutation completes.
    pub fn as_mut_string(&mut self) -> &mut String {
        &mut self.data
    }

    /// Append text, keeping the mlock on the (possibly reallocated)
    /// buffer. Used to assemble content (e.g. an existing note + an
    /// appended blurb) without it ever living in an unlocked `String`.
    pub fn push_str(&mut self, s: &str) {
        self.data.push_str(s);
        self.relock_if_moved();
    }

    /// If a mutation caused the String to reallocate (new address or
    /// capacity), move the mlock to the new region. The old region cannot
    /// be zeroized — it was already freed — which is why `EDIT_SLACK`
    /// exists to make this path rare.
    pub fn relock_if_moved(&mut self) {
        let ptr = self.data.as_ptr();
        let capacity = self.data.capacity();
        if ptr == self.locked_ptr && capacity == self.locked_len {
            return;
        }
        self.unlock_current();
        self.lock_current();
    }

    /// mlock the String's current full-capacity region and record it.
    fn lock_current(&mut self) {
        let ptr = self.data.as_ptr();
        let capacity = self.data.capacity();
        if capacity == 0 {
            self.locked_ptr = std::ptr::null();
            self.locked_len = 0;
            return;
        }
        let ok = unsafe { memory::mlock(ptr, capacity) };
        if ok {
            self.locked_ptr = ptr;
            self.locked_len = capacity;
        } else {
            self.locked_ptr = std::ptr::null();
            self.locked_len = 0;
            eprintln!("warning: could not mlock editable buffer — data may be swapped to disk");
        }
    }

    /// munlock the recorded region, if any.
    fn unlock_current(&mut self) {
        if !self.locked_ptr.is_null() && self.locked_len > 0 {
            unsafe {
                memory::munlock(self.locked_ptr, self.locked_len);
            }
        }
        self.locked_ptr = std::ptr::null();
        self.locked_len = 0;
    }
}

impl Drop for SecureString {
    fn drop(&mut self) {
        // Zeroize while the region is still locked (wipes full capacity),
        // then release the lock.
        unsafe {
            self.data.as_mut_vec().zeroize();
        }
        self.unlock_current();
    }
}

// SecureString must never leave the UI thread (its raw mlock-region
// pointers are only meaningful alongside the allocation they track, and
// all editing happens on the main thread), and must never be copyable,
// loggable, or printable. !Send/!Sync currently fall out of the raw
// pointer fields — these assertions keep that guarantee even if the
// representation changes (e.g. storing the lock region as usize would
// silently make it Send).
static_assertions::assert_not_impl_any!(
    SecureString: Send,
    Sync,
    Clone,
    std::fmt::Debug,
    std::fmt::Display
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secure_buffer_basic() {
        let data = vec![10, 20, 30, 40, 50];
        let buf = SecureBuffer::from_bytes(data);

        assert_eq!(buf.len(), 5);
        assert!(!buf.is_empty());
        assert_eq!(buf.as_bytes(), &[10, 20, 30, 40, 50]);
    }

    #[test]
    fn test_secure_buffer_str() {
        let message = "secret text";
        let buf = SecureBuffer::from_bytes(message.as_bytes().to_vec());

        assert_eq!(buf.as_str().unwrap(), "secret text");
    }

    #[test]
    fn test_secure_buffer_empty() {
        let buf = SecureBuffer::from_bytes(vec![]);

        assert!(buf.is_empty());
        assert_eq!(buf.len(), 0);
        assert_eq!(buf.as_bytes(), &[] as &[u8]);
    }

    #[test]
    fn test_secure_string_roundtrip() {
        let buf = SecureBuffer::from_bytes(b"hello world".to_vec());
        let s = SecureString::from_secure_buffer(&buf).unwrap();
        assert_eq!(s.as_str(), "hello world");
        assert_eq!(s.as_bytes(), b"hello world");
    }

    #[test]
    fn test_secure_string_push_str_relocks() {
        // Assemble content in secure memory (the quick-note append path):
        // start from an existing note, add a blurb, and force a
        // reallocation past the slack to exercise relock.
        let buf = SecureBuffer::from_bytes(b"existing note".to_vec());
        let mut s = SecureString::from_secure_buffer(&buf).unwrap();
        s.push_str("\n## header\n\nappended blurb\n");
        assert!(s.as_str().starts_with("existing note"));
        assert!(s.as_str().ends_with("appended blurb\n"));

        let big = "y".repeat(EDIT_SLACK + 4096);
        s.push_str(&big);
        assert_eq!(s.locked_ptr, s.as_str().as_ptr());
        assert!(s.as_str().ends_with("yyy"));
    }

    #[test]
    fn test_secure_string_edit_and_relock() {
        let buf = SecureBuffer::from_bytes(b"start".to_vec());
        let mut s = SecureString::from_secure_buffer(&buf).unwrap();

        s.as_mut_string().push_str(" + edited");
        s.relock_if_moved();
        assert_eq!(s.as_str(), "start + edited");

        // Force a reallocation past the slack capacity and verify the
        // lock follows the new allocation without crashing.
        let big = "x".repeat(EDIT_SLACK + 1024);
        s.as_mut_string().push_str(&big);
        s.relock_if_moved();
        assert!(s.as_str().ends_with("xxx"));
        assert_eq!(s.locked_ptr, s.data.as_ptr());
    }

    #[test]
    fn test_secure_string_empty() {
        let buf = SecureBuffer::from_bytes(vec![]);
        let s = SecureString::from_secure_buffer(&buf).unwrap();
        assert_eq!(s.as_str(), "");
    }
}
