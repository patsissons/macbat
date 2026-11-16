//! Minimal IOKit FFI surface used by the SMC reader and the battery registry reader.

#![allow(non_camel_case_types, non_upper_case_globals)]

use core_foundation::base::CFAllocatorRef;
use core_foundation::dictionary::{CFDictionaryRef, CFMutableDictionaryRef};
use std::ffi::{CStr, c_char, c_void};

pub type mach_port_t = u32;
pub type io_object_t = mach_port_t;
pub type io_iterator_t = io_object_t;
pub type io_service_t = io_object_t;
pub type io_registry_entry_t = io_object_t;
pub type io_connect_t = io_object_t;
pub type kern_return_t = i32;

pub const KERN_SUCCESS: kern_return_t = 0;
pub const kIOMainPortDefault: mach_port_t = 0;

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    static mach_task_self_: mach_port_t;

    pub fn IOServiceMatching(name: *const c_char) -> CFMutableDictionaryRef;
    pub fn IOServiceGetMatchingService(main: mach_port_t, matching: CFDictionaryRef) -> io_service_t;
    pub fn IOServiceGetMatchingServices(
        main: mach_port_t,
        matching: CFDictionaryRef,
        existing: *mut io_iterator_t,
    ) -> kern_return_t;
    pub fn IOIteratorNext(iterator: io_iterator_t) -> io_object_t;
    pub fn IOObjectRelease(object: io_object_t) -> kern_return_t;
    pub fn IORegistryEntryGetName(entry: io_registry_entry_t, name: *mut c_char) -> kern_return_t;
    pub fn IORegistryEntryCreateCFProperties(
        entry: io_registry_entry_t,
        properties: *mut CFMutableDictionaryRef,
        allocator: CFAllocatorRef,
        options: u32,
    ) -> kern_return_t;
    pub fn IOServiceOpen(
        service: io_service_t,
        owning_task: mach_port_t,
        kind: u32,
        connect: *mut io_connect_t,
    ) -> kern_return_t;
    pub fn IOServiceClose(connect: io_connect_t) -> kern_return_t;
    pub fn IOConnectCallStructMethod(
        connection: io_connect_t,
        selector: u32,
        input: *const c_void,
        input_size: usize,
        output: *mut c_void,
        output_size: *mut usize,
    ) -> kern_return_t;
}

pub fn mach_task_self() -> mach_port_t {
    unsafe { mach_task_self_ }
}

pub fn matching(class: &CStr) -> CFMutableDictionaryRef {
    unsafe { IOServiceMatching(class.as_ptr()) }
}

pub fn entry_name(entry: io_registry_entry_t) -> String {
    let mut buf = [0 as c_char; 128];
    unsafe {
        if IORegistryEntryGetName(entry, buf.as_mut_ptr()) != KERN_SUCCESS {
            return String::new();
        }
        CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned()
    }
}

/// Owns an IOKit object handle and releases it on drop.
pub struct IoObject(pub io_object_t);

impl Drop for IoObject {
    fn drop(&mut self) {
        if self.0 != 0 {
            unsafe { IOObjectRelease(self.0) };
        }
    }
}
