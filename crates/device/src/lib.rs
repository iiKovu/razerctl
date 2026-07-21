use anyhow::{bail, Context, Result};
use hidapi::{HidApi, HidDevice};
use std::time::{Duration, Instant};
use tracing::{debug, info};

use blackshark_protocol::{Report, REPORT_LEN};

const VID: u16 = 0x1532;
const PIDS: &[u16] = &[0x0577, 0x0a55];

fn is_supported_device(vid: u16, pid: u16) -> bool {
    vid == VID && PIDS.contains(&pid)
}

/// Open the BlackShark V3 Pro HID device.
///
/// Must open interface 5 specifically — the dongle exposes multiple HID interfaces
/// and api.open(VID, PID) picks the first enumerated, which varies across systems.
/// Interface 5 is the proprietary control interface (interrupt IN, endpoint 0x84).
pub fn open() -> Result<HidDevice> {
    let api = HidApi::new().context("failed to initialise hidapi")?;

    let mut target = None;
    for info in api.device_list() {
        if is_supported_device(info.vendor_id(), info.product_id()) {
            let path = info.path().to_string_lossy();
            info!(
                interface = info.interface_number(),
                path = %path,
                "found BlackShark hidraw interface"
            );
            if info.interface_number() == 5 {
                target = Some(info.clone());
            }
        }
    }

    match target {
        None => bail!("BlackShark V3 Pro (PC or Xbox edition) not found — is the dongle plugged in and do you have udev permission?"),
        Some(info) => {
            let path = info.path().to_string_lossy().into_owned();
            let dev = info
                .open_device(&api)
                .context("found BlackShark V3 Pro but failed to open control interface — check udev permissions")?;
            info!(path = %path, "opened BlackShark control interface");
            Ok(dev)
        }
    }
}

/// Send a report and read back the response.
///
/// Razer devices echo the command back with the status byte set.
pub fn send(dev: &HidDevice, report: &Report) -> Result<Report> {
    dev.write(report.as_bytes()).context("HID write failed")?;

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut last_rejection = None;

    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            if let Some(error) = last_rejection {
                bail!("timed out waiting for matching HID response; last rejected frame: {error}");
            }
            bail!("timed out waiting for HID response");
        }

        let mut buf = [0u8; REPORT_LEN];
        let timeout_ms = remaining.as_millis().clamp(1, i32::MAX as u128) as i32;
        let n = dev
            .read_timeout(&mut buf, timeout_ms)
            .context("HID read failed")?;

        if n == 0 {
            if let Some(error) = last_rejection {
                bail!("timed out waiting for matching HID response; last rejected frame: {error}");
            }
            bail!("timed out waiting for HID response");
        }
        if n != REPORT_LEN {
            bail!("short read: expected {REPORT_LEN} bytes, got {n}");
        }

        let response = Report::from_bytes(buf);

        match response.validate_response_to(report) {
            Ok(()) => return Ok(response),
            Err(error) => {
                debug!(response = ?response.as_bytes(), %error, "skipping non-matching HID frame");
                last_rejection = Some(error);
            }
        }
    }
}

#[cfg(test)]
fn first_matching_response(
    request: &Report,
    frames: impl IntoIterator<Item = Report>,
) -> Option<Report> {
    frames
        .into_iter()
        .find(|response| response.validate_response_to(request).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response_frame(request: &Report, class: u8, subcommand: u8) -> Report {
        let mut bytes = *request.as_bytes();
        bytes[1] = 0x02;
        bytes[10] = class;
        bytes[11] = subcommand;
        bytes[12] = 0x01;
        bytes[62] = bytes[..62]
            .iter()
            .fold(0u8, |checksum, byte| checksum ^ byte);
        Report::from_bytes(bytes)
    }

    #[test]
    fn supports_pc_and_xbox_wireless_dongles() {
        assert!(is_supported_device(0x1532, 0x0577));
        assert!(is_supported_device(0x1532, 0x0a55));
    }

    #[test]
    fn rejects_other_usb_devices() {
        assert!(!is_supported_device(0x1532, 0x0a4d));
        assert!(!is_supported_device(0x1234, 0x0a55));
    }

    #[test]
    fn skips_unsolicited_notification_before_matching_reply() {
        let request = Report::new(0x60, 0x21, 0x00, &[0x00]);
        let notification = response_frame(&request, 0x55, 0x02);
        let reply = response_frame(&request, 0x21, 0x01);

        let selected = first_matching_response(&request, [notification, reply]);

        assert_eq!(selected, Some(reply));
    }
}
