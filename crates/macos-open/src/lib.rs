//! Files opened from the Finder: a double-click, Open With, a drop on the Dock icon, or
//! `open -a`. macOS sends these as an "open documents" Apple Event, not as arguments.
//!
//! [`install`] listens for the app about to finish launching. That is where the handler must be
//! installed: after the system's default one (which it replaces), before the launch's own files
//! are delivered. The handler only queues paths; the app collects them with [`take`].
//!
//! This is the workspace's only `unsafe` code: a few Objective-C messages with no safe binding.
//! Elsewhere (and on other systems) the functions do nothing.

use std::path::PathBuf;
use std::sync::Mutex;

/// Paths opened from the Finder, waiting for the app.
static OPENED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
/// Wakes the app when a file arrives while it is idle.
static WAKE: Mutex<Option<Box<dyn Fn() + Send>>> = Mutex::new(None);

/// Starts listening. Call once, before the event loop runs.
pub fn install() {
    #[cfg(target_os = "macos")]
    macos::install();
}

/// Called whenever a file arrives, so an idle app redraws and collects it.
pub fn set_wake(wake: impl Fn() + Send + 'static) {
    if let Ok(mut slot) = WAKE.lock() {
        *slot = Some(Box::new(wake));
    }
}

/// The files opened since the last call, oldest first.
pub fn take() -> Vec<PathBuf> {
    OPENED.lock().map(|mut opened| std::mem::take(&mut *opened)).unwrap_or_default()
}

/// Adds opened files to the queue and wakes the app.
fn queue(paths: Vec<PathBuf>) {
    if paths.is_empty() {
        return;
    }
    if let Ok(mut opened) = OPENED.lock() {
        opened.extend(paths);
    }
    if let Ok(wake) = WAKE.lock()
        && let Some(wake) = wake.as_ref()
    {
        wake();
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::path::PathBuf;

    use objc2::rc::Retained;
    use objc2::runtime::{NSObject, Sel};
    use objc2::{AllocAnyThread, define_class, msg_send, sel};
    use objc2_foundation::{NSAppleEventDescriptor, NSAppleEventManager, NSNotification, NSNotificationCenter, NSString};

    /// `kCoreEventClass`, `kAEOpenDocuments` and `keyDirectObject`, from the Apple Event headers.
    const CORE_EVENT_CLASS: u32 = u32::from_be_bytes(*b"aevt");
    const OPEN_DOCUMENTS: u32 = u32::from_be_bytes(*b"odoc");
    const DIRECT_OBJECT: u32 = u32::from_be_bytes(*b"----");

    define_class!(
        #[unsafe(super(NSObject))]
        #[name = "MethodStorytellerOpenDocuments"]
        struct OpenDocuments;

        impl OpenDocuments {
            #[unsafe(method(willFinishLaunching:))]
            fn will_finish_launching(&self, _notification: &NSNotification) {
                let manager = NSAppleEventManager::sharedAppleEventManager();
                let handler: Sel = sel!(handleOpenDocuments:withReplyEvent:);
                // SAFETY: the selector names a method this class defines with the signature the
                // manager calls, `self` lives for the whole process (see `install`), and the
                // event class and ID are `AEEventClass`/`AEEventID`, both 32-bit unsigned.
                let () = unsafe {
                    msg_send![&manager, setEventHandler: self, andSelector: handler, forEventClass: CORE_EVENT_CLASS, andEventID: OPEN_DOCUMENTS]
                };
            }

            #[unsafe(method(handleOpenDocuments:withReplyEvent:))]
            fn handle_open_documents(&self, event: Option<&NSAppleEventDescriptor>, _reply: Option<&NSAppleEventDescriptor>) {
                let Some(event) = event else { return };
                // SAFETY: `paramDescriptorForKeyword:` takes an `AEKeyword` (32-bit unsigned) and
                // returns a descriptor or nil.
                let list: Option<Retained<NSAppleEventDescriptor>> = unsafe { msg_send![event, paramDescriptorForKeyword: DIRECT_OBJECT] };
                let Some(list) = list else { return };
                let mut paths = Vec::new();
                let count = list.numberOfItems();
                if count == 0 {
                    // One file, sent as a single descriptor rather than a list.
                    if let Some(path) = list.fileURLValue().and_then(|url| url.path()) {
                        paths.push(PathBuf::from(path.to_string()));
                    }
                }
                // Apple Event lists count from 1.
                for index in 1..=count {
                    let path = list.descriptorAtIndex(index).and_then(|item| item.fileURLValue()).and_then(|url| url.path());
                    if let Some(path) = path {
                        paths.push(PathBuf::from(path.to_string()));
                    }
                }
                super::queue(paths);
            }
        }
    );

    pub(super) fn install() {
        // SAFETY: `init` on a freshly allocated NSObject subclass with no instance variables.
        let observer: Retained<OpenDocuments> = unsafe { msg_send![OpenDocuments::alloc(), init] };
        let name = NSString::from_str("NSApplicationWillFinishLaunchingNotification");
        // SAFETY: the observer implements the selector with one NSNotification argument, and it
        // is never released (below), so the centre never messages a freed object.
        unsafe {
            NSNotificationCenter::defaultCenter().addObserver_selector_name_object(&observer, sel!(willFinishLaunching:), Some(&name), None);
        }
        // The notification centre and the Apple Event manager do not keep their targets alive;
        // the observer lives as long as the app.
        std::mem::forget(observer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opened_files_queue_in_order_and_wake_the_app() {
        let woken = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = woken.clone();
        set_wake(move || {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        });
        queue(vec![PathBuf::from("/a.osf"), PathBuf::from("/b.fountain")]);
        queue(Vec::new());
        assert_eq!(take(), [PathBuf::from("/a.osf"), PathBuf::from("/b.fountain")]);
        assert!(take().is_empty(), "taken once");
        assert_eq!(woken.load(std::sync::atomic::Ordering::SeqCst), 1, "an empty event wakes nothing");
    }
}
