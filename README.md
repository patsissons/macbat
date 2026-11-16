# macbat

Sudoless power monitor for Apple Silicon laptops, in the style of [macmon](https://github.com/vladkens/macmon).
Shows what the power adapter is delivering, what the system is drawing, and whether the battery is
charging or covering a shortfall — live, and over time.

```
╭ 8 W adapter · 5.0 V × 1.50 A ──────────────────────────── Mac14,2 · macbat v0.1.0 ╮
│╭ Power In ──────────────── 7.15 W ╮╭ System ───────────────────────────── 3.77 W ╮│
││                     ▂▂▂▂▂▂▂▂▂▂▂▂ ││                       █▃█▂▃▄▁▂▂▂▃▄          ││
│╰ 4.99 V × 1.43 A ── 89% of 8 W ───╯╰ avg 3.81 W · max 5.72 W ──────────────────╯│
│╭ Battery ────── +3.41 W charging ╮╭ Charge ────────────────────────────── 90% ╮│
││                    ▆ ▆▆▄█▆▆▆▅▃▄ ││██████████████████████90% ████████▎        ││
│╰ 12.66 V × +0.27 A ─ full in 1:49 ╯╰ 3489 / 3881 mAh ── 88% of design · 181 cycles ╯│
│╭ Power History ─────────────── avg in 7.15 W · system 3.81 W · battery +3.37 W ╮│
││ braille line chart of in / system / battery (battery goes below 0 W when draining) ││
│╰ since start: in 0.02 Wh · system 0.01 Wh · battery +0.01 Wh ──────────────────╯│
╰──────────────────── 'q' quit · 'c' color · 'v' view: sparkline · '+/-' history: 1m ╯
```

## Install

```sh
cargo install --path .
```

## Usage

```sh
macbat                 # TUI
macbat -i 500          # TUI, sample every 500 ms
macbat pipe            # JSON lines, one sample per interval
macbat pipe -s 1 | jq  # single sample
macbat debug           # raw SMC keys + battery registry entry
macbat debug P         # every SMC key starting with "P"
```

Keys: `q` quit · `c` cycle color · `v` sparkline/gauge view · `+`/`-` history window (1m, 5m, 15m, 1h).
Preferences persist in `~/.config/macbat.json`.

## Where the numbers come from

| Metric | Source | Notes |
|---|---|---|
| Power in (W, V, A) | SMC `PDTR`, `VD0R`, `ID0R` | DC input after the USB-C/MagSafe port, ~1 s refresh |
| System power | SMC `PSTR` | Whole-system draw (same value macmon calls `sys_power`) |
| Battery V / A | SMC `B0AV`, `B0AC` | Little-endian ints on Apple Silicon; A is signed (− = discharging). Cross-checked against the registry voltage |
| Battery power | `B0AV × B0AC` | + charging, − discharging |
| Battery temp | SMC `TB0T`–`TB2T` | Average of the pack sensors |
| Adapter rating, charge %, mAh, cycles, time to full/empty | IORegistry `AppleSmartBattery` | Same data as `ioreg -rw0 -c AppleSmartBattery` |

The `AppleSmartBattery` registry entry also has `PowerTelemetryData` (system power in/load, battery
power), but it only refreshes every ~30–60 s, so it is used only as a fallback when the SMC is
unavailable (the header then says "registry only").

Sensors are read at slightly different moments, so on a single sample `in ≈ system + battery`
can be off by a watt or two during load spikes; the averages in the history panel line up.

"% of design" is `NominalChargeCapacity / DesignCapacity`. macOS's "Maximum Capacity" in System
Settings uses its own (unpublished) formula and may differ by a few percent.
