//! Sudoless reader for the Apple System Management Controller (SMC).
//!
//! The SMC exposes live power sensors (DC-in voltage/current, system power, battery
//! power) that refresh far more often than the battery driver's registry properties.

use crate::iokit::*;
use std::ffi::c_void;
use std::mem::size_of;

const KERNEL_INDEX_SMC: u32 = 2;
const CMD_READ_BYTES: u8 = 5;
const CMD_READ_INDEX: u8 = 8;
const CMD_READ_KEYINFO: u8 = 9;

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
struct KeyDataVer {
    major: u8,
    minor: u8,
    build: u8,
    reserved: u8,
    release: u16,
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
struct PLimitData {
    version: u16,
    length: u16,
    cpu_p_limit: u32,
    gpu_p_limit: u32,
    mem_p_limit: u32,
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct KeyInfo {
    pub data_size: u32,
    pub data_type: u32,
    pub data_attributes: u8,
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
struct KeyData {
    key: u32,
    vers: KeyDataVer,
    p_limit_data: PLimitData,
    key_info: KeyInfo,
    result: u8,
    status: u8,
    data8: u8,
    data32: u32,
    bytes: [u8; 32],
}

pub fn fourcc(s: &str) -> u32 {
    s.bytes().take(4).fold(0u32, |acc, b| (acc << 8) | b as u32)
}

pub fn fourcc_str(v: u32) -> String {
    v.to_be_bytes().iter().map(|&b| b as char).collect()
}

pub struct Smc {
    conn: io_connect_t,
}

impl Smc {
    pub fn open() -> Result<Self, String> {
        let mut iter: io_iterator_t = 0;
        let kr = unsafe {
            IOServiceGetMatchingServices(kIOMainPortDefault, matching(c"AppleSMC") as _, &mut iter)
        };
        if kr != KERN_SUCCESS {
            return Err(format!("IOServiceGetMatchingServices(AppleSMC) failed: {kr:#x}"));
        }
        let iter = IoObject(iter);

        loop {
            let device = unsafe { IOIteratorNext(iter.0) };
            if device == 0 {
                return Err("AppleSMCKeysEndpoint not found".into());
            }
            let device = IoObject(device);
            if entry_name(device.0) != "AppleSMCKeysEndpoint" {
                continue;
            }
            let mut conn: io_connect_t = 0;
            let kr = unsafe { IOServiceOpen(device.0, mach_task_self(), 0, &mut conn) };
            if kr != KERN_SUCCESS {
                return Err(format!("IOServiceOpen(AppleSMC) failed: {kr:#x}"));
            }
            return Ok(Self { conn });
        }
    }

    fn call(&self, input: &KeyData) -> Result<KeyData, String> {
        let mut output = KeyData::default();
        let mut out_size = size_of::<KeyData>();
        let kr = unsafe {
            IOConnectCallStructMethod(
                self.conn,
                KERNEL_INDEX_SMC,
                input as *const _ as *const c_void,
                size_of::<KeyData>(),
                &mut output as *mut _ as *mut c_void,
                &mut out_size,
            )
        };
        if kr != KERN_SUCCESS {
            return Err(format!("SMC call failed: {kr:#x}"));
        }
        if output.result != 0 {
            return Err(format!("SMC result {}", output.result));
        }
        Ok(output)
    }

    pub fn key_info(&self, key: u32) -> Result<KeyInfo, String> {
        let input = KeyData { key, data8: CMD_READ_KEYINFO, ..Default::default() };
        Ok(self.call(&input)?.key_info)
    }

    pub fn read_raw(&self, key: u32) -> Result<(KeyInfo, Vec<u8>), String> {
        let info = self.key_info(key)?;
        let mut input = KeyData { key, data8: CMD_READ_BYTES, ..Default::default() };
        input.key_info.data_size = info.data_size;
        let out = self.call(&input)?;
        let n = (info.data_size as usize).min(32);
        Ok((info, out.bytes[..n].to_vec()))
    }

    /// Reads a key and decodes it as a number, if its type is numeric.
    pub fn read_f64(&self, key: &str) -> Option<f64> {
        let (info, bytes) = self.read_raw(fourcc(key)).ok()?;
        decode(info.data_type, &bytes)
    }

    pub fn key_count(&self) -> u32 {
        self.read_raw(fourcc("#KEY"))
            .ok()
            .and_then(|(_, b)| b.get(..4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]])))
            .unwrap_or(0)
    }

    pub fn key_at(&self, index: u32) -> Result<u32, String> {
        let input = KeyData { data8: CMD_READ_INDEX, data32: index, ..Default::default() };
        Ok(self.call(&input)?.key)
    }
}

impl Drop for Smc {
    fn drop(&mut self) {
        unsafe { IOServiceClose(self.conn) };
    }
}

/// Decodes the common numeric SMC data types. Apple Silicon uses little-endian
/// `flt ` and `ioft`; the legacy integer types are big-endian.
pub fn decode(data_type: u32, b: &[u8]) -> Option<f64> {
    let t = fourcc_str(data_type);
    match t.as_str() {
        "flt " if b.len() >= 4 => Some(f32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64),
        "ioft" if b.len() >= 8 => {
            let v = u64::from_le_bytes(b[..8].try_into().ok()?);
            Some(v as f64 / 65536.0)
        }
        "ui8 " if !b.is_empty() => Some(b[0] as f64),
        "si8 " if !b.is_empty() => Some(b[0] as i8 as f64),
        "ui16" if b.len() >= 2 => Some(u16::from_be_bytes([b[0], b[1]]) as f64),
        "si16" if b.len() >= 2 => Some(i16::from_be_bytes([b[0], b[1]]) as f64),
        "ui32" if b.len() >= 4 => Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as f64),
        "si32" if b.len() >= 4 => Some(i32::from_be_bytes([b[0], b[1], b[2], b[3]]) as f64),
        _ => None,
    }
}
