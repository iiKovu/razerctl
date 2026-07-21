use std::time::{Duration, Instant};

use anyhow::Result;
use hidapi::HidDevice;
use tokio::sync::{mpsc, oneshot, watch};
use tracing::{debug, info, warn};

use blackshark_device as device;
use blackshark_protocol::{cmd, Report};

use crate::config::Config;
use crate::state::SharedState;

// ---------------------------------------------------------------------------
// Public command API
// ---------------------------------------------------------------------------

pub struct BatteryState {
    pub percentage: u8,
    pub charging: bool,
}

pub enum HidCommand {
    SetSidetone {
        level: u8,
        reply: oneshot::Sender<Result<()>>,
    },
    GetBattery {
        reply: oneshot::Sender<Result<BatteryState>>,
    },
    SetThx {
        enabled: bool,
        reply: oneshot::Sender<Result<()>>,
    },
    SetAnc {
        enabled: bool,
        level: u8,
        reply: oneshot::Sender<Result<()>>,
    },
    SetPowerSavings {
        minutes: u8,
        reply: oneshot::Sender<Result<()>>,
    },
    SetEq {
        preset: u8,
        reply: oneshot::Sender<Result<()>>,
    },
    /// Sent when config changes — restores all settings to the device.
    ApplyConfig { config: Config },
    /// Periodic wakeup sent by a tokio timer — drives reconnect + battery poll.
    Tick,
}

// ---------------------------------------------------------------------------
// Actor entry point
// ---------------------------------------------------------------------------

const BATTERY_POLL_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// Spawn the HID actor on a dedicated OS thread.
///
/// `HidDevice` is not `Send`, so all HID I/O stays on this thread.
/// Communication with async callers is via the mpsc channel + oneshot replies.
pub fn spawn(
    rx: mpsc::Receiver<HidCommand>,
    state_tx: watch::Sender<SharedState>,
    initial_config: Config,
) {
    std::thread::Builder::new()
        .name("hid-actor".into())
        .spawn(move || run(rx, state_tx, initial_config))
        .expect("failed to spawn hid-actor thread");
}

fn run(
    mut rx: mpsc::Receiver<HidCommand>,
    state_tx: watch::Sender<SharedState>,
    initial_config: Config,
) {
    let mut reconnect_config = initial_config;
    let mut dev: Option<HidDevice> = try_open();
    let mut next_battery_poll = Instant::now(); // poll immediately on first tick
    let mut device_ready = false; // true after first successful battery poll
    let mut rf_wait_count: u32 = 0;

    while let Some(cmd) = rx.blocking_recv() {
        match cmd {
            HidCommand::Tick => {
                if dev.is_none() {
                    if let Some(d) = try_open() {
                        dev = Some(d);
                        device_ready = false;
                    }
                }
                if Instant::now() >= next_battery_poll {
                    if let Some(d) = &dev {
                        match query_battery(d) {
                            Ok(b) => {
                                next_battery_poll = Instant::now() + BATTERY_POLL_INTERVAL;
                                debug!(
                                    percentage = b.percentage,
                                    charging = b.charging,
                                    "battery poll"
                                );
                                if !device_ready {
                                    // A matched battery reply proves the control channel is usable.
                                    device_ready = true;
                                    rf_wait_count = 0;
                                    let sidetone = query_sidetone(d).ok();
                                    info!(percentage = b.percentage, sidetone, "headset connected");
                                    state_tx.send_modify(|s| {
                                        s.connected = true;
                                        s.battery_pct = b.percentage;
                                        s.charging = b.charging;
                                        if let Some(v) = sidetone {
                                            s.sidetone = v;
                                        }
                                    });
                                    let outcome = restore_config(d, &reconnect_config);
                                    state_tx.send_modify(|state| {
                                        apply_restore_outcome(state, &reconnect_config, outcome)
                                    });
                                } else {
                                    state_tx.send_modify(|s| {
                                        s.battery_pct = b.percentage;
                                        s.charging = b.charging;
                                    });
                                }
                            }
                            Err(e) => {
                                if device_ready {
                                    warn!("headset disconnected: {e}");
                                    device_ready = false;
                                    dev = None;
                                    rf_wait_count = 0;
                                    state_tx.send_modify(|s| s.connected = false);
                                } else {
                                    // Clear the device handle so try_open() fires next Tick.
                                    // This handles the case where the dongle is unplugged
                                    // before a valid control reply arrives — without this, the
                                    // stale HidDevice handle prevents reconnect detection.
                                    dev = None;
                                    rf_wait_count += 1;
                                    if rf_wait_count == 1 || rf_wait_count.is_multiple_of(6) {
                                        info!(attempt = rf_wait_count, error = %e, "dongle present but control interface is not responding");
                                    }
                                }
                            }
                        }
                    }
                }
            }

            HidCommand::SetSidetone { level, reply } => {
                info!(level, "set_sidetone");
                let result = with_dev(&mut dev, &state_tx, |d| set_sidetone(d, level));
                match &result {
                    Ok(()) => {
                        info!(level, "set_sidetone ok");
                        state_tx.send_modify(|s| s.sidetone = level);
                    }
                    Err(e) => warn!("set_sidetone failed: {e}"),
                }
                let _ = reply.send(result);
            }

            HidCommand::SetThx { enabled, reply } => {
                info!(enabled, "set_thx");
                let result = with_dev(&mut dev, &state_tx, |d| set_thx(d, enabled));
                match &result {
                    Ok(()) => {
                        info!(enabled, "set_thx ok");
                        state_tx.send_modify(|s| s.thx_enabled = enabled);
                    }
                    Err(e) => warn!("set_thx failed: {e}"),
                }
                let _ = reply.send(result);
            }

            HidCommand::SetAnc {
                enabled,
                level,
                reply,
            } => {
                info!(enabled, level, "set_anc");
                let result = with_dev(&mut dev, &state_tx, |d| set_anc(d, enabled, level));
                match &result {
                    Ok(()) => {
                        info!(enabled, level, "set_anc ok");
                        state_tx.send_modify(|s| {
                            s.anc_enabled = enabled;
                            s.anc_level = level;
                        });
                    }
                    Err(e) => warn!("set_anc failed: {e}"),
                }
                let _ = reply.send(result);
            }

            HidCommand::SetPowerSavings { minutes, reply } => {
                info!(minutes, "set_power_savings");
                let result = with_dev(&mut dev, &state_tx, |d| set_power_savings(d, minutes));
                match &result {
                    Ok(()) => {
                        info!(minutes, "set_power_savings ok");
                        state_tx.send_modify(|s| s.power_savings_minutes = minutes);
                    }
                    Err(e) => warn!("set_power_savings failed: {e}"),
                }
                let _ = reply.send(result);
            }

            HidCommand::SetEq { preset, reply } => {
                info!(preset, "set_eq");
                let result = with_dev(&mut dev, &state_tx, |d| set_eq_preset(d, preset));
                match &result {
                    Ok(()) => {
                        info!(preset, "set_eq ok");
                        state_tx.send_modify(|s| s.eq_preset = preset);
                    }
                    Err(e) => warn!("set_eq failed: {e}"),
                }
                let _ = reply.send(result);
            }

            HidCommand::GetBattery { reply } => {
                info!("get_battery");
                let result = with_dev(&mut dev, &state_tx, query_battery);
                match &result {
                    Ok(b) => {
                        info!(
                            percentage = b.percentage,
                            charging = b.charging,
                            "get_battery ok"
                        );
                        state_tx.send_modify(|s| {
                            s.battery_pct = b.percentage;
                            s.charging = b.charging;
                        });
                    }
                    Err(e) => warn!("get_battery failed: {e}"),
                }
                let _ = reply.send(result);
            }

            HidCommand::ApplyConfig { config } => {
                replace_reconnect_config(&mut reconnect_config, config);
                if let Some(d) = &dev {
                    let outcome = restore_config(d, &reconnect_config);
                    state_tx.send_modify(|state| {
                        apply_restore_outcome(state, &reconnect_config, outcome)
                    });
                }
            }
        }
    }
}

fn replace_reconnect_config(current: &mut Config, updated: Config) {
    *current = updated;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applied_config_becomes_the_reconnect_config() {
        let mut current = Config::default();
        let mut updated = current.clone();
        updated.sidetone = 42;
        updated.anc_enabled = true;

        replace_reconnect_config(&mut current, updated);

        assert_eq!(current.sidetone, 42);
        assert!(current.anc_enabled);
    }

    #[test]
    fn restore_status_only_reports_successful_commands() {
        let config = Config {
            sidetone: 7,
            eq_preset: 3,
            thx_enabled: true,
            anc_enabled: true,
            anc_level: 4,
            power_savings_minutes: 30,
        };
        let mut state = SharedState::default();
        let outcome = RestoreOutcome {
            sidetone: true,
            eq: false,
            thx: true,
            anc: false,
            power_savings: true,
        };

        apply_restore_outcome(&mut state, &config, outcome);

        assert_eq!(state.sidetone, 7);
        assert_eq!(state.eq_preset, 0);
        assert!(state.thx_enabled);
        assert!(!state.anc_enabled);
        assert_eq!(state.power_savings_minutes, 30);
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Open the hidraw device. A later matched battery response distinguishes a usable
/// control channel from an unlinked headset or the dongle's audio-only fallback.
fn try_open() -> Option<HidDevice> {
    match device::open() {
        Err(_) => None,
        Ok(d) => {
            info!("dongle opened, waiting for a valid control response");
            Some(d)
        }
    }
}

/// Apply all config values to the device. Logs but does not fail on errors —
/// best-effort restore so a single bad command doesn't block the rest.
#[derive(Clone, Copy, Debug, Default)]
struct RestoreOutcome {
    sidetone: bool,
    eq: bool,
    thx: bool,
    anc: bool,
    power_savings: bool,
}

fn restore_config(dev: &HidDevice, config: &Config) -> RestoreOutcome {
    info!(
        sidetone = config.sidetone,
        thx = config.thx_enabled,
        anc = config.anc_enabled,
        power_savings = config.power_savings_minutes,
        "restoring config to device"
    );

    let mut outcome = RestoreOutcome::default();
    match set_sidetone(dev, config.sidetone) {
        Ok(()) => outcome.sidetone = true,
        Err(e) => warn!("restore sidetone failed: {e}"),
    }
    match set_eq_preset(dev, config.eq_preset) {
        Ok(()) => outcome.eq = true,
        Err(e) => warn!("restore eq failed: {e}"),
    }
    match set_thx(dev, config.thx_enabled) {
        Ok(()) => outcome.thx = true,
        Err(e) => warn!("restore thx failed: {e}"),
    }
    match set_anc(dev, config.anc_enabled, config.anc_level) {
        Ok(()) => outcome.anc = true,
        Err(e) => warn!("restore anc failed: {e}"),
    }
    match set_power_savings(dev, config.power_savings_minutes) {
        Ok(()) => outcome.power_savings = true,
        Err(e) => warn!("restore power_savings failed: {e}"),
    }
    outcome
}

fn apply_restore_outcome(state: &mut SharedState, config: &Config, outcome: RestoreOutcome) {
    if outcome.sidetone {
        state.sidetone = config.sidetone;
    }
    if outcome.eq {
        state.eq_preset = config.eq_preset;
    }
    if outcome.thx {
        state.thx_enabled = config.thx_enabled;
    }
    if outcome.anc {
        state.anc_enabled = config.anc_enabled;
        state.anc_level = config.anc_level;
    }
    if outcome.power_savings {
        state.power_savings_minutes = config.power_savings_minutes;
    }
}

/// Run `f` with the current device, clearing it on I/O failure.
fn with_dev<T, F>(
    dev: &mut Option<HidDevice>,
    state_tx: &watch::Sender<SharedState>,
    f: F,
) -> Result<T>
where
    F: FnOnce(&HidDevice) -> Result<T>,
{
    match dev {
        None => anyhow::bail!("headset not connected"),
        Some(d) => {
            let result = f(d);
            if result.is_err() {
                warn!("headset disconnected");
                *dev = None;
                state_tx.send_modify(|s| s.connected = false);
            }
            result
        }
    }
}

// ---------------------------------------------------------------------------
// HID operations
// ---------------------------------------------------------------------------

fn set_sidetone(dev: &HidDevice, level: u8) -> Result<()> {
    let get = Report::new(
        0x60,
        cmd::SIDETONE_GET_CLASS,
        cmd::SIDETONE_ID,
        &[cmd::SIDETONE_GET_ARG, 0x00],
    );
    device::send(dev, &get)?;
    let set = Report::new(
        0x60,
        cmd::SIDETONE_SET_CLASS,
        cmd::SIDETONE_ID,
        &[level, 0x00],
    );
    device::send(dev, &set)?;
    Ok(())
}

fn query_battery(dev: &HidDevice) -> Result<BatteryState> {
    let report = Report::new(0x60, cmd::BATTERY_CLASS, cmd::BATTERY_ID, &[0x00]);
    let response = device::send(dev, &report)?;
    let args = response.args();
    anyhow::ensure!(args.len() >= 2, "battery response too short");
    anyhow::ensure!(
        args[0] <= 100,
        "battery percentage out of range: {}",
        args[0]
    );
    Ok(BatteryState {
        percentage: args[0],
        charging: args[1] != 0x00,
    })
}

fn set_thx(dev: &HidDevice, enabled: bool) -> Result<()> {
    let mode = if enabled {
        cmd::THX_SPATIAL
    } else {
        cmd::THX_STEREO
    };
    let report = Report::new(0x60, cmd::THX_CLASS, cmd::THX_ID, &[mode, 0x00]);
    device::send(dev, &report)?;
    Ok(())
}

fn set_anc(dev: &HidDevice, enabled: bool, level: u8) -> Result<()> {
    let level = level.clamp(cmd::ANC_LEVEL_MIN, cmd::ANC_LEVEL_MAX);
    let report = Report::new(
        0x60,
        cmd::ANC_CLASS,
        cmd::ANC_ID,
        &[enabled as u8, level, 0x00],
    );
    device::send(dev, &report)?;
    Ok(())
}

fn set_power_savings(dev: &HidDevice, minutes: u8) -> Result<()> {
    let report = Report::new(
        0x60,
        cmd::POWER_SAVINGS_CLASS,
        cmd::POWER_SAVINGS_ID,
        &[minutes, 0x00],
    );
    device::send(dev, &report)?;
    Ok(())
}

/// Band data per preset (confirmed from Synapse pcap captures).
/// Format: [preset_idx, b0..b8, extra, padding] — 12 bytes total.
/// Band values use sign-magnitude encoding: 0x00=0dB, 0x01=+1dB, 0x81=−1dB.
/// Bands: 60Hz, 170Hz, 310Hz, 600Hz, 1kHz, 3kHz, 6kHz, 12kHz, 16kHz.
const EQ_BANDS: [[u8; 12]; 5] = [
    [
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ], // 0: Flat
    [
        0x01, 0x02, 0x02, 0x05, 0x05, 0x01, 0x81, 0x02, 0x03, 0x03, 0x03, 0x00,
    ], // 1
    [
        0x02, 0x03, 0x03, 0x03, 0x81, 0x84, 0x84, 0x02, 0x03, 0x03, 0x03, 0x00,
    ], // 2
    [
        0x03, 0x02, 0x02, 0x00, 0x00, 0x01, 0x81, 0x81, 0x03, 0x03, 0x03, 0x00,
    ], // 3
    [
        0x04, 0x01, 0x01, 0x81, 0x00, 0x02, 0x00, 0x04, 0x04, 0x04, 0x83, 0x00,
    ], // 4
];

/// Meta args per preset (7 bytes, from captures).
const EQ_META: [[u8; 7]; 5] = [
    [0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00],
    [0x01, 0x01, 0x01, 0x00, 0x01, 0x00, 0x00],
    [0x02, 0x03, 0x01, 0x00, 0x03, 0x00, 0x00],
    [0x03, 0x02, 0x01, 0x00, 0x02, 0x00, 0x00],
    [0x04, 0x04, 0x01, 0x00, 0x0b, 0x00, 0x00],
];

/// Commit args per preset (12 bytes, from captures).
const EQ_COMMIT: [[u8; 12]; 5] = [
    [
        0x00, 0x00, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ],
    [
        0x01, 0x00, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ],
    [
        0x02, 0x00, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ],
    [
        0x03, 0x00, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ],
    [
        0x04, 0x00, 0x00, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ],
];

fn set_eq_preset(dev: &HidDevice, preset: u8) -> Result<()> {
    anyhow::ensure!(
        (preset as usize) < EQ_BANDS.len(),
        "preset index out of range (0–4)"
    );

    let idx = preset as usize;
    let (bands, meta, commit) = (EQ_BANDS[idx], EQ_META[idx], EQ_COMMIT[idx]);

    // 1. GET current state
    device::send(
        dev,
        &Report::new(0x60, cmd::EQ_STATE_CLASS, cmd::EQ_STATE_ID, &[0x01, 0x00]),
    )?;

    // 2. SET bands
    device::send(
        dev,
        &Report::new(0x60, cmd::EQ_BANDS_CLASS, cmd::EQ_BANDS_ID, &bands),
    )?;

    // 3. SET meta
    device::send(
        dev,
        &Report::new(0x60, cmd::EQ_META_CLASS, cmd::EQ_META_ID, &meta),
    )?;

    // 4. APPLY
    device::send(
        dev,
        &Report::new(0x60, cmd::EQ_STATE_CLASS, cmd::EQ_STATE_ID, &[0x02, 0x00]),
    )?;

    // 5. COMMIT
    device::send(
        dev,
        &Report::new(0x60, cmd::EQ_COMMIT_CLASS, cmd::EQ_COMMIT_ID, &commit),
    )?;

    Ok(())
}

fn query_sidetone(dev: &HidDevice) -> Result<u8> {
    let report = Report::new(0x60, cmd::SIDETONE_READ_CLASS, 0x00, &[0x00]);
    let response = device::send(dev, &report)?;
    let args = response.args();
    anyhow::ensure!(!args.is_empty(), "sidetone response empty");
    anyhow::ensure!(
        args[0] <= cmd::SIDETONE_MAX,
        "sidetone out of range: {}",
        args[0]
    );
    Ok(args[0])
}
