//! EXACT (EXACT-PATCHES.md, 17): one process-wide registration shared by
//! every holder, kept by the handle its registration returned.

/// The holders of one registration and its handle (0: none registered).
pub(crate) struct Shared {
    holders: usize,
    handle: usize,
}

impl Shared {
    pub(crate) const fn new() -> Self {
        Self {
            holders: 0,
            handle: 0,
        }
    }

    /// A holder joins: `add` registers, answering the handle or 0 when it
    /// failed, when nothing is registered (a failed registration is tried
    /// again by the next holder).
    pub(crate) fn register(&mut self, add: impl FnOnce() -> usize) {
        if self.handle == 0 {
            self.handle = add();
        }
        self.holders += 1;
    }

    /// A holder leaves: the last calls `remove` with the handle `add`
    /// returned, when there is one.
    pub(crate) fn unregister(&mut self, remove: impl FnOnce(usize)) {
        self.holders = self.holders.saturating_sub(1);
        if self.holders == 0 && self.handle != 0 {
            remove(core::mem::take(&mut self.handle));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Shared;
    use alloc::vec::Vec;

    /// What the system holds: the handles registered and not removed.
    #[derive(Default)]
    struct System {
        registered: Vec<usize>,
        next: usize,
    }

    impl System {
        fn add(&mut self) -> usize {
            self.next += 1;
            self.registered.push(self.next + 0x1000);
            self.next + 0x1000
        }
        fn remove(&mut self, handle: usize) {
            let at = self.registered.iter().position(|h| *h == handle);
            self.registered.remove(at.expect("a handle add returned"));
        }
    }

    #[test]
    fn devices_made_and_dropped_leave_no_handler_registered() {
        let (mut shared, mut system) = (Shared::new(), System::default());
        for _ in 0..3 {
            shared.register(|| system.add());
            assert_eq!(system.registered.len(), 1);
            shared.unregister(|h| system.remove(h));
            assert!(system.registered.is_empty());
        }
        shared.register(|| system.add());
        shared.register(|| system.add());
        assert_eq!(system.registered.len(), 1, "one handler for every device");
        shared.unregister(|h| system.remove(h));
        assert_eq!(system.registered.len(), 1, "a device still holds it");
        shared.unregister(|h| system.remove(h));
        assert!(system.registered.is_empty());
    }

    #[test]
    fn a_failed_registration_removes_nothing_and_is_tried_again() {
        let (mut shared, mut system) = (Shared::new(), System::default());
        shared.register(|| 0);
        shared.register(|| system.add());
        assert_eq!(system.registered.len(), 1);
        shared.unregister(|h| system.remove(h));
        shared.unregister(|h| system.remove(h));
        assert!(system.registered.is_empty());
        shared.register(|| 0);
        shared.unregister(|_| panic!("nothing was registered"));
    }
}
