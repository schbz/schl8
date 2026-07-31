/// Install a panic hook that ensures clean shutdown.
/// In GUI mode, there is no terminal to restore — the default panic
/// handler prints to stderr and the process exits normally, running
/// Drop handlers (which zeroize SecureBuffer memory).
pub fn install_panic_hook() {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        original_hook(panic_info);
    }));
}
