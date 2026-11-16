# AGENTS.md

Guidance for coding agents working on macbat, a sudoless power monitor TUI for Apple Silicon
laptops modelled on [macmon](https://github.com/vladkens/macmon). See README.md for user-facing
docs and the metric → sensor table.

## Commands

```sh
cargo build                       # debug build
cargo build --release             # LTO + stripped, ~1.2 MB
scripts/build.sh [--debug]        # same, release by default; prints the binary path and size
scripts/install.sh                # release build, installed to $BINDIR (default ~/.local/bin)
scripts/run.sh [args]             # cargo run (debug), args go to macbat
scripts/screenshot.sh             # regenerate assets/screenshot.svg (README image) from a live run
./target/debug/macbat pipe -s 3   # 3 JSON samples — quickest end-to-end check of the data layer
./target/debug/macbat debug       # raw SMC keys, registry entry, and one combined sample
./target/debug/macbat debug P     # every SMC key starting with "P" (use to discover sensors)
```

There is no test suite; the data comes from live hardware. Verify changes by running the binary.
`cargo clippy` is not installed on the local toolchain.

### Checking the TUI without a terminal

The TUI needs a real tty. Render it headless in tmux and capture the screen:

```sh
tmux new-session -d -s macbat -x 120 -y 36 ./target/debug/macbat
sleep 5; tmux capture-pane -p -t macbat      # text snapshot of the UI
tmux send-keys -t macbat v                    # drive keys: q c v + -
tmux kill-session -t macbat
```

Pressing `c`/`v`/`+`/`-` writes `~/.config/macbat.json` (the user's real preferences). Restore it
after testing.

## Layout

| File | Role |
|---|---|
| `src/iokit.rs` | Hand-written IOKit FFI (no `io-kit-sys` dependency); `IoObject` releases on drop |
| `src/smc.rs` | SMC client over `AppleSMCKeysEndpoint`: key info, raw reads, key enumeration, type decoding |
| `src/battery.rs` | Reads the `AppleSmartBattery` registry entry, serializes it to a binary plist, deserializes with serde |
| `src/sampler.rs` | Merges SMC + registry into `Sample` (the JSON schema for `pipe`); all fallback logic lives here |
| `src/app.rs` | ratatui UI: panels, sparkline/gauge views, history chart, key handling |
| `src/config.rs` | Persisted UI prefs (color, view, history window) |
| `src/main.rs` | clap CLI: default TUI, `pipe`, `debug` |
| `scripts/` | `build.sh`, `install.sh` and `run.sh` wrappers around cargo; `screenshot.sh` + `ansi2svg.py` render the README image |

The TUI samples on a background thread and sends `Sample`s over an mpsc channel; the UI thread
keeps up to 1 h of history in a `VecDeque`.

## Hardware data gotchas

These were found by probing an M2 MacBook Air (Mac14,2). Re-check with `macbat debug` before
relying on them for other models.

- **The registry's power telemetry is stale.** `PowerTelemetryData` (SystemPowerIn, SystemLoad,
  BatteryPower) only refreshes every ~30–60 s. Live values must come from the SMC (~1 s refresh).
  The registry is still the source for adapter rating, charge %, mAh, cycles and time estimates.
- **SMC integer endianness is mixed.** `flt ` keys are little-endian. Integer keys vary per key:
  `B0AV`/`B0AC` are little-endian, while e.g. `D1MV`/`D1MI` read correctly as big-endian.
  `smc::decode` assumes big-endian ints. Read specific keys with explicit byte order (see
  `Sampler::read_smc`), and `debug` prints both interpretations.
- **The SMC's float battery keys are unsigned.** `PPBR`, `Pb0f` and `Ib0f` are magnitudes and
  don't match charging power. Signed battery power is `B0AV × B0AC` (mV × mA, − = discharging).
  The sampler only trusts this when `B0AV` is within 1.5 V of the registry voltage, and otherwise
  falls back to the registry.
- **Power balance:** `PDTR` (adapter in) ≈ `PSTR` (system) + battery charge power. `PSTR` and
  `B0AC` are sampled at slightly different moments, so a single sample can be off by a watt or
  two during spikes. That's expected, so don't "fix" it by smoothing the raw data.
- **Registry integers are signed.** Negative values such as discharge `InstantAmperage` show up
  as huge unsigned numbers in `ioreg` text output, but they deserialize correctly as `i64`.
- **No published "Maximum Capacity".** macOS's battery health percentage isn't stored in the
  registry. The UI shows `NominalChargeCapacity / DesignCapacity`, labelled "% of design". Don't
  call it health.
- `AvgTimeToFull` / `AvgTimeToEmpty` use 65535 to mean "still estimating".

## Conventions

- Keep it sudoless; that's a core goal, shared with macmon.
- Keep the macmon look: rounded borders, the name in the left of the title, the live value in the
  right of the title, details in the bottom border, one accent color cycled with `c`.
- `Sample` is a public JSON contract for `pipe`. Add fields rather than renaming or removing them.
- Commits: short imperative subject, a body explaining why, and each commit should build on its
  own.
