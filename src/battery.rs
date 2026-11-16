//! Reads the `AppleSmartBattery` IORegistry entry (the same data `ioreg -c AppleSmartBattery`
//! prints). It is the source for adapter ratings, charge level and capacity. Its power
//! telemetry only refreshes every ~30–60 s, so live power numbers come from the SMC.

use crate::iokit::*;
use core_foundation::base::{TCFType, kCFAllocatorDefault};
use core_foundation::dictionary::{CFDictionary, CFMutableDictionaryRef};
use core_foundation::propertylist::{create_data, kCFPropertyListBinaryFormat_v1_0};
use serde::Deserialize;

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct Registry {
    pub external_connected: bool,
    pub is_charging: bool,
    pub fully_charged: bool,
    /// State of charge, percent.
    pub current_capacity: i64,
    /// Battery voltage, mV.
    pub voltage: i64,
    /// Battery current, mA (negative while discharging).
    pub instant_amperage: i64,
    /// Minutes; 65535 while macOS is still estimating.
    pub avg_time_to_full: i64,
    pub avg_time_to_empty: i64,
    pub cycle_count: i64,
    pub adapter_details: Option<AdapterDetails>,
    pub battery_data: Option<BatteryData>,
    pub power_telemetry_data: Option<Telemetry>,
    pub charger_data: Option<ChargerData>,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct AdapterDetails {
    pub watts: i64,
    /// mV
    pub adapter_voltage: i64,
    /// mA
    pub current: i64,
    pub is_wireless: bool,
    pub name: Option<String>,
    pub description: Option<String>,
    /// The USB-PD voltage/current levels the charger offered (its source capabilities).
    pub usb_hvc_menu: Vec<HvcOffer>,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct HvcOffer {
    /// mV
    pub max_voltage: i64,
    /// mA
    pub max_current: i64,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct BatteryData {
    pub design_capacity: i64,
    pub nominal_charge_capacity: i64,
    pub full_charge_capacity: i64,
    pub remaining_capacity: i64,
}

/// All power values in mW, voltage in mV, current in mA.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct Telemetry {
    pub system_power_in: i64,
    pub system_voltage_in: i64,
    pub system_current_in: i64,
    pub system_load: i64,
    pub battery_power: i64,
    pub adapter_efficiency_loss: i64,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct ChargerData {
    pub not_charging_reason: i64,
    pub slow_charging_reason: i64,
}

pub struct BatteryReader {
    entry: IoObject,
}

impl BatteryReader {
    pub fn open() -> Result<Self, String> {
        let entry =
            unsafe { IOServiceGetMatchingService(kIOMainPortDefault, matching(c"AppleSmartBattery") as _) };
        if entry == 0 {
            return Err("AppleSmartBattery not found (no internal battery?)".into());
        }
        Ok(Self { entry: IoObject(entry) })
    }

    pub fn read(&self) -> Result<Registry, String> {
        let mut props: CFMutableDictionaryRef = std::ptr::null_mut();
        let kr = unsafe {
            IORegistryEntryCreateCFProperties(self.entry.0, &mut props, kCFAllocatorDefault, 0)
        };
        if kr != KERN_SUCCESS || props.is_null() {
            return Err(format!("IORegistryEntryCreateCFProperties failed: {kr:#x}"));
        }
        // Take ownership so the dictionary is released when we are done.
        let dict: CFDictionary = unsafe { CFDictionary::wrap_under_create_rule(props as _) };
        let data = create_data(dict.as_concrete_TypeRef() as _, kCFPropertyListBinaryFormat_v1_0)
            .map_err(|e| format!("serialize battery properties: {e}"))?;
        plist::from_bytes(data.bytes()).map_err(|e| format!("parse battery properties: {e}"))
    }
}
