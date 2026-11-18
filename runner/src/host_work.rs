//! What a data source can hand the host's executor that no grant names (LLP
//! 1047.001 D12): a [`crate::Request::continuation`], which compiles only for
//! a source that declares [`HostWork`]. The bake reads the declaration
//! ([`hands_out_work!`]) and links the executor, which is the host's I/O.

/// A source that hands the host native work: [`crate::Request::continuation`]
/// requires it. A source names itself, `Request::continuation::<Self>(token)`,
/// and a wrapper that forwards a child's continuations implements it when the
/// child does. The type is not checked to be the answering source: one that
/// names another type, or fills the field by hand, goes unseen by the bake,
/// and an Apple host without I/O refuses its work by name.
pub trait HostWork {}

/// A build script may name its source by reference.
impl<T: HostWork + ?Sized> HostWork for &T {}
impl<T: HostWork + ?Sized> HostWork for &mut T {}

/// A native module's call, which a host's executor runs: an Apple
/// composition that links modules links I/O with them.
pub(crate) struct ModuleWork;
impl HostWork for ModuleWork {}

/// A stream's held outcome sent back out as a turn: the stream is a request
/// its source's grants name, which link I/O already.
pub(crate) struct StreamTurn;
impl HostWork for StreamTurn {}

/// The host work the runner's tests hand out.
#[cfg(test)]
pub(crate) struct TestWork;
#[cfg(test)]
impl HostWork for TestWork {}

/// Whether a value's type declares [`HostWork`], for a build script's concrete
/// type, which stable Rust can only ask by method resolution: `&&Probe` finds
/// [`Declares`] when the type implements the marker, else derefs to
/// [`Undeclared`].
#[doc(hidden)]
pub struct Probe<T: ?Sized>(std::marker::PhantomData<*const T>);

impl<T: ?Sized> Probe<T> {
    #[doc(hidden)]
    pub fn of(_: &T) -> Self {
        Probe(std::marker::PhantomData)
    }
}

#[doc(hidden)]
pub trait Declares {
    fn hands_out_work(&self) -> bool {
        true
    }
}

impl<T: HostWork + ?Sized> Declares for &Probe<T> {}

#[doc(hidden)]
pub trait Undeclared {
    fn hands_out_work(&self) -> bool {
        false
    }
}

impl<T: ?Sized> Undeclared for Probe<T> {}

/// Whether `source`'s type declares [`HostWork`]: what a build script passes
/// the bake beside the source's grants. It reads the type the call site sees,
/// so ask it of a value of the source's concrete type: in a generic function
/// whose bounds do not name `HostWork` it answers `false` whatever the
/// parameter, and a tuple struct's bare name is its constructor function,
/// which declares nothing.
#[macro_export]
macro_rules! hands_out_work {
    ($source:expr) => {{
        #[allow(unused_imports)]
        use $crate::host_work::{Declares as _, Undeclared as _};
        (&&$crate::host_work::Probe::of(&$source)).hands_out_work()
    }};
}

#[cfg(test)]
mod tests {
    struct Plain;
    struct Working;
    impl super::HostWork for Working {}

    #[test]
    fn the_probe_reads_the_marker_of_a_concrete_type() {
        assert!(!hands_out_work!(Plain));
        assert!(hands_out_work!(Working));
        let mut working = Working;
        assert!(hands_out_work!(&working));
        assert!(hands_out_work!(&mut working));
    }
}
