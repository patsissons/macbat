//! Combines live SMC sensors with the battery registry into one `Sample`.

use crate::battery::{BatteryReader, Registry};
use crate::smc::{Smc, fourcc};
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Default, Serialize)]
pub struct Sample {
    pub timestamp_ms: u64,
    /// "smc" when live sensors were available, "registry" when falling back.
    pub source: &'static str,
    pub external_connected: bool,
    pub input: Input,
    /// Total system power draw, W.
    pub system_watts: f64,
    pub battery: Battery,
    pub adapter: Option<Adapter>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Input {
    pub watts: f64,
    pub volts: f64,
    pub amps: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Battery {
    /// Positive while charging, negative while discharging.
    pub watts: f64,
    pub volts: f64,
    pub amps: f64,
    pub percent: f64,
    pub state: &'static str,
    /// Minutes to full while charging, to empty while discharging.
    pub minutes_remaining: Option<u32>,
    pub temp_c: Option<f64>,
    pub cycle_count: Option<u32>,
    pub remaining_mah: Option<u32>,
    pub full_charge_mah: Option<u32>,
    pub nominal_mah: Option<u32>,
    pub design_mah: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Adapter {
    /// Power negotiated with the charger, W. Limited by what the charger offers (which the
    /// cable can cap at 3 A) and by what the Mac accepts, so it can be below the label rating.
    pub rated_watts: f64,
    /// Negotiated voltage, V.
    pub volts: f64,
    /// Negotiated current, A.
    pub amps: f64,
    pub name: Option<String>,
    pub wireless: bool,
    /// Conversion loss reported by the battery driver, W (refreshes slowly).
    pub loss_watts: Option<f64>,
    /// Voltage/current levels the charger offered, lowest voltage first.
    pub offers: Vec<Offer>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Offer {
    pub volts: f64,
    pub amps: f64,
}

struct SmcReading {
    in_w: f64,
    in_v: f64,
    in_a: f64,
    sys_w: f64,
    batt_v: Option<f64>,
    batt_a: Option<f64>,
    temp_c: Option<f64>,
}

pub struct Sampler {
    smc: Option<Smc>,
    battery: Option<BatteryReader>,
}

impl Sampler {
    pub fn new() -> Result<Self, String> {
        let smc = Smc::open().ok();
        let battery = BatteryReader::open().ok();
        if smc.is_none() && battery.is_none() {
            return Err("neither the SMC nor the battery registry could be opened".into());
        }
        Ok(Self { smc, battery })
    }

    fn read_smc(&self) -> Option<SmcReading> {
        let smc = self.smc.as_ref()?;
        let f = |k: &str| smc.read_f64(k);
        // B0AV/B0AC are little-endian integers on Apple Silicon (mV / signed mA).
        let le16 = |k: &str| {
            smc.read_raw(fourcc(k)).ok().filter(|(i, _)| i.data_size == 2).map(|(_, b)| [b[0], b[1]])
        };
        let temps: Vec<f64> = ["TB0T", "TB1T", "TB2T"].iter().filter_map(|k| f(k)).filter(|t| *t > 0.0).collect();
        Some(SmcReading {
            in_w: f("PDTR")?,
            in_v: f("VD0R").unwrap_or(0.0),
            in_a: f("ID0R").unwrap_or(0.0),
            sys_w: f("PSTR")?,
            batt_v: le16("B0AV").map(|b| u16::from_le_bytes(b) as f64 / 1000.0),
            batt_a: le16("B0AC").map(|b| i16::from_le_bytes(b) as f64 / 1000.0),
            temp_c: (!temps.is_empty()).then(|| temps.iter().sum::<f64>() / temps.len() as f64),
        })
    }

    pub fn sample(&mut self) -> Sample {
        let reg: Option<Registry> = self.battery.as_ref().and_then(|b| b.read().ok());
        let smc = self.read_smc();
        let tel = reg.as_ref().and_then(|r| r.power_telemetry_data.clone()).unwrap_or_default();
        let external = reg.as_ref().map(|r| r.external_connected).unwrap_or_else(|| {
            smc.as_ref().is_some_and(|s| s.in_v > 1.0)
        });

        let input = match (&smc, external) {
            (_, false) => Input::default(),
            (Some(s), _) => Input { watts: s.in_w, volts: s.in_v, amps: s.in_a },
            (None, _) => Input {
                watts: tel.system_power_in as f64 / 1000.0,
                volts: tel.system_voltage_in as f64 / 1000.0,
                amps: tel.system_current_in as f64 / 1000.0,
            },
        };

        // Battery V/A: prefer the SMC, but only if its voltage agrees with the registry
        // (guards against a different byte order on other hardware).
        let reg_v = reg.as_ref().map(|r| r.voltage as f64 / 1000.0);
        let reg_a = reg.as_ref().map(|r| r.instant_amperage as f64 / 1000.0);
        let (batt_v, batt_a) = match smc.as_ref().and_then(|s| s.batt_v.zip(s.batt_a)) {
            Some((v, a)) if reg_v.is_none_or(|rv| (rv - v).abs() < 1.5) => (v, a),
            _ => (reg_v.unwrap_or(0.0), reg_a.unwrap_or(0.0)),
        };
        let batt_w = batt_v * batt_a;

        let system_watts = match &smc {
            Some(s) => s.sys_w,
            None if tel.system_load > 0 => tel.system_load as f64 / 1000.0,
            None => (input.watts - batt_w).max(0.0),
        };

        let fully_charged = reg.as_ref().is_some_and(|r| r.fully_charged);
        let state = if batt_a > 0.01 {
            "charging"
        } else if batt_a < -0.01 {
            "discharging"
        } else if fully_charged {
            "full"
        } else {
            "idle"
        };
        let minutes = |m: i64| (m > 0 && m < 65535).then_some(m as u32);
        let minutes_remaining = reg.as_ref().and_then(|r| match state {
            "charging" => minutes(r.avg_time_to_full),
            "discharging" => minutes(r.avg_time_to_empty),
            _ => None,
        });
        let bd = reg.as_ref().and_then(|r| r.battery_data.clone()).unwrap_or_default();
        let mah = |v: i64| (v > 0).then_some(v as u32);

        let adapter = reg.as_ref().and_then(|r| r.adapter_details.clone()).filter(|_| external).map(|a| Adapter {
            rated_watts: a.watts as f64,
            volts: a.adapter_voltage as f64 / 1000.0,
            amps: a.current as f64 / 1000.0,
            // The registry pads some names, e.g. "70W USB-C Power Adapter ".
            name: [a.name, a.description].into_iter().flatten().map(|s| s.trim().to_string()).find(|s| !s.is_empty()),
            wireless: a.is_wireless,
            loss_watts: (tel.adapter_efficiency_loss > 0).then(|| tel.adapter_efficiency_loss as f64 / 1000.0),
            offers: a
                .usb_hvc_menu
                .iter()
                .map(|o| Offer { volts: o.max_voltage as f64 / 1000.0, amps: o.max_current as f64 / 1000.0 })
                .collect(),
        });

        Sample {
            timestamp_ms: SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0),
            source: if smc.is_some() { "smc" } else { "registry" },
            external_connected: external,
            input,
            system_watts,
            battery: Battery {
                watts: batt_w,
                volts: batt_v,
                amps: batt_a,
                percent: reg.as_ref().map(|r| r.current_capacity as f64).unwrap_or(0.0),
                state,
                minutes_remaining,
                temp_c: smc.as_ref().and_then(|s| s.temp_c),
                cycle_count: reg.as_ref().and_then(|r| mah(r.cycle_count)),
                remaining_mah: mah(bd.remaining_capacity),
                full_charge_mah: mah(bd.full_charge_capacity),
                nominal_mah: mah(bd.nominal_charge_capacity),
                design_mah: mah(bd.design_capacity),
            },
            adapter,
        }
    }
}
