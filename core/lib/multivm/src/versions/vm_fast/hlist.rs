//! # Heterogeneous lists
//! A HList is an array that can contain elements of different types.
//! At runtime it works like a struct but at compile time it is more convenient.
//! The tracer interface is automatically implements the `Tracer` trait for tuple constructions like this.

use std::fmt::{self, DebugList, Formatter};

macro_rules! hlist {
    [] => ();
    [$head:tt] => {
        ($head, ())
    };
    [$head:tt, $($tail:tt),*] => {
        ($head, hlist![$($tail),*])
    };
}
pub(crate) use hlist;

pub(crate) fn debug_hlist<T: DebugHList>(hlist: &T, f: &mut Formatter<'_>) -> fmt::Result {
    let mut list = f.debug_list();
    hlist.debug(&mut list);
    list.finish()
}

pub(crate) trait DebugHList {
    fn debug(&self, f: &mut DebugList<'_, '_>);
}

impl<T: fmt::Debug, W: DebugHList> DebugHList for (T, W) {
    fn debug(&self, list: &mut DebugList<'_, '_>) {
        list.entry(&self.0);
        self.1.debug(list)
    }
}

impl DebugHList for () {
    fn debug(&self, _: &mut DebugList<'_, '_>) {}
}

pub(crate) struct Here;
pub(crate) struct Later<T>(T);
pub(crate) trait HListGet<T, W> {
    fn get(&mut self) -> &mut T;
}

impl<T, U> HListGet<T, Here> for (T, U) {
    fn get(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T, W, U, V: HListGet<T, W>> HListGet<T, Later<W>> for (U, V) {
    fn get(&mut self) -> &mut T {
        self.1.get()
    }
}
