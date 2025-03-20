// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

use core::{marker::PhantomData, mem::MaybeUninit, ptr};

use wdk_sys::{
    call_unsafe_wdf_function_binding,
    NTSTATUS,
    PCUNICODE_STRING,
    PDRIVER_OBJECT,
    WDF_DRIVER_CONFIG,
    WDF_OBJECT_ATTRIBUTES,
};

use crate::wdf::Object;
use crate::nt_success;

pub type Driver<'a> = Object<'a, wdk_sys::WDFDRIVER>;

// typedef void *HANDLE; //this isnt actually used in strict mode (default)
//
// #define DECLARE_HANDLE(name) struct name##__{int unused;}; typedef struct
// name##__ *name
//
// DECLARE_HANDLE(WDFDRIVER)
// struct WDFDRIVER__{
//   int unused;
// };
// typedef struct WDFDRIVER__ *WDFDRIVER;

impl<'a> Driver<'a> {
    // TODO: wrap DriverEntry args used in driver entry callback with rust wrapper
    // types TODO: Result-type ntstatus
    #[doc(alias = "WdfDriverCreate")]
    pub unsafe fn try_new(
        driver: PDRIVER_OBJECT,
        registry_path: PCUNICODE_STRING,
        mut attributes: Option<WDF_OBJECT_ATTRIBUTES>,
        mut config: WDF_DRIVER_CONFIG,
    ) -> Result<Self, NTSTATUS> {
        let mut wdf_driver = MaybeUninit::uninit();

        let nt_status = unsafe {
            call_unsafe_wdf_function_binding!(
                WdfDriverCreate,
                driver,
                registry_path,
                attributes.as_mut().map_or(ptr::null_mut(), |attributes| attributes),
                &mut config,
                wdf_driver.as_mut_ptr(),
            )
        };
        nt_success(nt_status)
            .then(|| unsafe {
                Self {
                    data: [],
                    marker: PhantomData,
                    inner: wdf_driver.assume_init(),
                }
            })
            .ok_or(nt_status)
    }
    // TODO: Driver::create alias to help people unfamiliar with rust naming
    // conventions (but familiar with C WDK)
}

// todo: need generic over owned vs not owned
// #[derive(Debug)]
// pub struct Driver<'a, T: ?Sized> {
//     inner: wdk_sys::WDFDRIVER,
//     phantom: PhantomData<&'a mut T>,
// }

// impl<'a, T: ?Sized> WdfMemory<'a, T> {
//     // TODO: safety about type of T needing to match WDFMEMORY allocation
//     pub unsafe fn from_raw(raw: wdk_sys::WDFMEMORY) -> Self {
//         WdfMemory {
//             inner: raw,
//             phantom: PhantomData,
//         }
//     }
// }

#[cfg(test)]
mod tests {
    use wdk_sys::WDFMEMORY;

    use super::*;

    // Happy paths:
    // TODO: show retrieving a concrete primitive type
    // TODO: show retrieving a custom struct and accessing parts of it
    // TODO: show retrieving a slice (ie. this is a DST so it is an unsized
    // type) TODO: show retrieving a array (ie fixed size)

    // Edge cases
    // TODO: show mutable aliasing is not possible
    // TODO: show concurrent immutable accesses via shared references
    // TODO: show borrowing of individual parts of the Struct (if the wdfmemory
    // shows a struct) TODO: show owned vs. non-owned freeing
    // TODO: show mismatch of buffer size and num elemets in slice cases
}
