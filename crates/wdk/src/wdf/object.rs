// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

/// Generic type used to enforce variance and auto-traits on all WDF Object
/// types.
#[repr(C)]
pub(crate) struct Object<'a, T> {
    // Required for FFI-safe 0-sized type to
    // TODO: By including at least one private field and no constructor, we create an opaque type
    // that we can't instantiate outside of this module. (A struct with no field could be
    // instantiated by anyone.) We also want to use this type in FFI, so we have to add #[repr(C)]
    //
    // In the future, this should refer to an extern type.
    // See https://github.com/rust-lang/rust/issues/43467.
    pub(crate) data: [u8; 0],

    // Required for !Send & !Sync & !Unpin.
    //
    // - `*mut u8` is !Send & !Sync. It must be in `PhantomData` to not affect alignment.
    //
    // - `PhantomPinned` is !Unpin. It must be in `PhantomData` because its memory representation
    //   is not considered FFI-safe.
    pub(crate) marker: core::marker::PhantomData<(*mut u8, core::marker::PhantomPinned, &'a T)>,

    pub(crate) inner: T,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// F<T> is covariant over T if T being a subtype of U implies that F<T> is
    /// a subtype of F<U>
    fn test_covariance<'short, 'long: 'short>() {
        let long: Object<&'long u8> = Object {
            _data: [],
            _marker: core::marker::PhantomData,
            inner: &0,
        };

        let short: Object<&'short u8> = long;

        // get size
        assert_eq!(core::mem::size_of_val(&short), core::mem::size_of::<&u8>());
    }

    // TODO: negative contra test
}
