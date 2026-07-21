use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use serde::Serialize;
use zbus::Connection;

use blackshark_client::HeadsetProxy;

#[derive(Parser)]
#[command(
    name = "blackshark-ctl",
    version,
    about = "Control the Razer BlackShark V3 Pro headset"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Set a captured EQ preset (0=flat, 1-4 named presets)
    Eq {
        #[arg(value_name = "PRESET", value_parser = clap::value_parser!(u8).range(0..=4))]
        preset: u8,
    },
    /// Set sidetone level (0–15)
    Sidetone {
        #[arg(value_name = "LEVEL", value_parser = clap::value_parser!(u8).range(0..=15))]
        level: u8,
    },
    /// Query battery level
    Battery,
    /// Toggle THX Spatial Audio (on/off)
    Thx {
        #[arg(value_name = "on|off", value_parser = parse_bool, action = clap::ArgAction::Set)]
        enabled: bool,
    },
    /// Set Active Noise Cancellation
    Anc {
        #[arg(value_name = "on|off", value_parser = parse_bool, action = clap::ArgAction::Set)]
        enabled: bool,
        /// ANC strength level (1–4)
        #[arg(value_name = "LEVEL", default_value = "4",
              value_parser = clap::value_parser!(u8).range(1..=4))]
        level: u8,
    },
    /// Set power savings timeout
    PowerSavings {
        /// Minutes before auto-shutoff (0=off, 15, 30, 45, 60)
        #[arg(value_name = "MINUTES", value_parser = clap::builder::PossibleValuesParser::new(["0","15","30","45","60"]))]
        minutes: String,
    },
    /// Print full device status as JSON (useful for waybar / scripts)
    Status,
    /// Subscribe to all device signals and print changes as they arrive
    Monitor,
}

fn parse_bool(s: &str) -> Result<bool, String> {
    match s {
        "on" | "true" | "1" => Ok(true),
        "off" | "false" | "0" => Ok(false),
        _ => Err(format!("expected on/off, got '{s}'")),
    }
}

fn requires_connected(command: &Command) -> bool {
    !matches!(command, Command::Status | Command::Monitor)
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let conn = Connection::session().await?;
    let proxy = HeadsetProxy::new(&conn).await?;

    if requires_connected(&cli.command) && !proxy.connected().await? {
        bail!("headset is not connected (is blacksharkd running?)");
    }

    match cli.command {
        Command::Eq { preset } => {
            proxy.set_eq(preset).await?;
            println!("EQ preset set to {preset}");
        }
        Command::Sidetone { level } => {
            proxy.set_sidetone(level).await?;
            println!("sidetone set to {level}");
        }
        Command::Battery => {
            let (pct, charging) = proxy.get_battery().await?;
            let charging = if charging { " (charging)" } else { "" };
            println!("battery: {pct}%{charging}");
        }
        Command::Thx { enabled } => {
            proxy.set_thx(enabled).await?;
            println!("THX Spatial: {}", if enabled { "on" } else { "off" });
        }
        Command::Anc { enabled, level } => {
            proxy.set_anc(enabled, level).await?;
            println!(
                "ANC: {} (level {level})",
                if enabled { "on" } else { "off" }
            );
        }
        Command::PowerSavings { minutes } => {
            let m: u8 = minutes.parse().unwrap();
            proxy.set_power_savings(m).await?;
            println!(
                "power savings: {}",
                if m == 0 {
                    "off".to_string()
                } else {
                    format!("{m} min")
                }
            );
        }
        Command::Status => cmd_status(&proxy).await?,
        Command::Monitor => cmd_monitor(&proxy).await?,
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// status --json
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct Status {
    connected: bool,
    battery_percentage: u8,
    charging: bool,
    eq_preset: u8,
    sidetone: u8,
    thx_enabled: bool,
    anc_enabled: bool,
    anc_level: u8,
    power_savings_minutes: u8,
}

async fn cmd_status(proxy: &HeadsetProxy<'_>) -> Result<()> {
    let status = Status {
        connected: proxy.connected().await?,
        battery_percentage: proxy.battery_percentage().await?,
        charging: proxy.charging().await?,
        eq_preset: proxy.eq_preset().await?,
        sidetone: proxy.sidetone().await?,
        thx_enabled: proxy.thx_enabled().await?,
        anc_enabled: proxy.anc_enabled().await?,
        anc_level: proxy.anc_level().await?,
        power_savings_minutes: proxy.power_savings_minutes().await?,
    };
    println!("{}", serde_json::to_string_pretty(&status)?);
    Ok(())
}

// ---------------------------------------------------------------------------
// monitor
// ---------------------------------------------------------------------------

async fn cmd_monitor(proxy: &HeadsetProxy<'_>) -> Result<()> {
    use futures_util::StreamExt;

    eprintln!("monitoring — press Ctrl+C to stop");

    // Print current state first so there's always a baseline.
    if proxy.connected().await? {
        let (pct, charging) = proxy.get_battery().await?;
        println!(
            "connected  battery={}% charging={} sidetone={}",
            pct,
            charging,
            proxy.sidetone().await?
        );
    } else {
        println!("disconnected");
    }

    let mut battery_stream = proxy.receive_battery_changed().await?;
    let mut connected_stream = proxy.receive_connected_changed().await;
    let mut sidetone_stream = proxy.receive_sidetone_changed().await;

    loop {
        tokio::select! {
            Some(sig) = battery_stream.next() => {
                let args = sig.args()?;
                println!("battery_changed  percentage={}  charging={}", args.percentage, args.charging);
            }
            Some(change) = connected_stream.next() => {
                let val = change.get().await?;
                println!("connected_changed  connected={val}");
            }
            Some(change) = sidetone_stream.next() => {
                let val = change.get().await?;
                println!("sidetone_changed  sidetone={val}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_json_exposes_charging_state() {
        let status = Status {
            connected: true,
            battery_percentage: 31,
            charging: true,
            eq_preset: 0,
            sidetone: 5,
            thx_enabled: true,
            anc_enabled: true,
            anc_level: 4,
            power_savings_minutes: 15,
        };

        let json = serde_json::to_value(status).unwrap();
        assert_eq!(json["charging"], true);
    }

    #[test]
    fn rejects_uncaptured_eq_placeholders() {
        assert!(Cli::try_parse_from(["blackshark-ctl", "eq", "4"]).is_ok());
        assert!(Cli::try_parse_from(["blackshark-ctl", "eq", "5"]).is_err());
    }

    #[test]
    fn accepts_boolean_values_for_thx_and_anc() {
        assert!(Cli::try_parse_from(["blackshark-ctl", "thx", "on"]).is_ok());
        assert!(Cli::try_parse_from(["blackshark-ctl", "thx", "off"]).is_ok());
        assert!(Cli::try_parse_from(["blackshark-ctl", "anc", "on", "4"]).is_ok());
        assert!(Cli::try_parse_from(["blackshark-ctl", "anc", "off", "4"]).is_ok());
    }

    #[test]
    fn status_does_not_require_an_active_headset_link() {
        assert!(!requires_connected(&Command::Status));
        assert!(!requires_connected(&Command::Monitor));
        assert!(requires_connected(&Command::Battery));
    }
}
