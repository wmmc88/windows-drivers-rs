// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

use core::{marker::PhantomData, mem::MaybeUninit, ptr};

use wdk_sys::{
    call_unsafe_wdf_function_binding,
    NTSTATUS,
    WDFDEVICE_INIT,
    WDF_OBJECT_ATTRIBUTES,
};

use crate::wdf::Object;
use crate::nt_success;

pub type Device<'a> = Object<'a, wdk_sys::WDFDEVICE>;

impl<'a> Device<'a> {
    #[doc(alias = "WdfDeviceCreate")]
    pub unsafe fn try_new(
        mut device_init: *mut WDFDEVICE_INIT,
        mut attributes: Option<WDF_OBJECT_ATTRIBUTES>,
    ) -> Result<Self, NTSTATUS> {
        let mut wdf_device = MaybeUninit::uninit();

        let nt_status = unsafe {
            call_unsafe_wdf_function_binding!(
                WdfDeviceCreate,
                (core::ptr::addr_of_mut!(device_init)),
                attributes.as_mut().map_or(ptr::null_mut(), |attributes| attributes),
                wdf_device.as_mut_ptr(),
            )
        };
        nt_success(nt_status)
            .then(|| unsafe {
                Self {
                    data: [],
                    marker: PhantomData,
                    inner: wdf_device.assume_init(),
                }
            })
            .ok_or(nt_status)
    }
}
