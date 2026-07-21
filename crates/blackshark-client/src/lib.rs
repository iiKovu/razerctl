/// Display names for the five presets backed by captured device data.
pub const EQ_PRESET_NAMES: [&str; 5] = ["Default", "Game", "Movie", "Music", "Counter-Strike 2"];

/// Typed D-Bus proxy for the blacksharkd Headset interface.
///
/// zbus generates the implementation from this trait definition —
/// method/property names map 1:1 to the interface declared in the daemon.
#[zbus::proxy(
    interface = "net.blackshark1.Headset",
    default_service = "net.blackshark1",
    default_path = "/net/blackshark1/Headset"
)]
pub trait Headset {
    /// Set a captured EQ preset (0–4). Preset 0 = flat.
    fn set_eq(&self, preset: u8) -> zbus::Result<()>;

    /// Set sidetone level (0–15).
    fn set_sidetone(&self, level: u8) -> zbus::Result<()>;

    /// Enable or disable THX Spatial Audio.
    fn set_thx(&self, enabled: bool) -> zbus::Result<()>;

    /// Set Active Noise Cancellation. level must be 1–4.
    fn set_anc(&self, enabled: bool, level: u8) -> zbus::Result<()>;

    /// Set power savings auto-shutoff. minutes: 0 (off), 15, 30, 45, or 60.
    fn set_power_savings(&self, minutes: u8) -> zbus::Result<()>;

    /// Returns (percentage, charging).
    fn get_battery(&self) -> zbus::Result<(u8, bool)>;

    /// Whether the headset is currently reachable.
    #[zbus(property)]
    fn connected(&self) -> zbus::Result<bool>;

    /// Cached battery percentage.
    #[zbus(property)]
    fn battery_percentage(&self) -> zbus::Result<u8>;

    /// Whether the headset reports that its battery is charging.
    #[zbus(property)]
    fn charging(&self) -> zbus::Result<bool>;

    /// Cached sidetone level (0–15).
    #[zbus(property)]
    fn sidetone(&self) -> zbus::Result<u8>;

    #[zbus(property)]
    fn eq_preset(&self) -> zbus::Result<u8>;

    #[zbus(property)]
    fn thx_enabled(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn anc_enabled(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn anc_level(&self) -> zbus::Result<u8>;

    #[zbus(property)]
    fn power_savings_minutes(&self) -> zbus::Result<u8>;

    /// Emitted when the battery level changes.
    #[zbus(signal)]
    fn battery_changed(&self, percentage: u8, charging: bool) -> zbus::Result<()>;
}
