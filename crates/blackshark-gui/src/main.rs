use anyhow::Result;
use blackshark_client::HeadsetProxy;
use slint::ComponentHandle;
use zbus::Connection;

slint::include_modules!();

fn show_error(window: slint::Weak<MainWindow>, message: String) {
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(window) = window.upgrade() {
            window.set_last_error(message.into());
        }
    });
}

async fn load_state(window: &MainWindow, proxy: &HeadsetProxy<'_>) -> zbus::Result<()> {
    let connected = proxy.connected().await?;
    window.set_connected(connected);
    window.set_battery_pct(proxy.battery_percentage().await? as i32);
    window.set_charging(proxy.charging().await?);
    window.set_eq_preset(proxy.eq_preset().await? as i32);
    window.set_sidetone(proxy.sidetone().await? as i32);
    window.set_thx_enabled(proxy.thx_enabled().await?);
    window.set_anc_enabled(proxy.anc_enabled().await?);
    window.set_anc_level(proxy.anc_level().await? as i32);
    window.set_power_savings(proxy.power_savings_minutes().await? as i32);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let connection = Connection::session().await?;
    let proxy = HeadsetProxy::new(&connection).await?;
    let window = MainWindow::new()?;

    if let Err(error) = load_state(&window, &proxy).await {
        window.set_last_error(format!("Could not load headset state: {error}").into());
    }

    {
        let connection = connection.clone();
        let weak = window.as_weak();
        window.on_set_eq(move |preset| {
            let connection = connection.clone();
            let weak = weak.clone();
            tokio::spawn(async move {
                let result = async {
                    HeadsetProxy::new(&connection)
                        .await?
                        .set_eq(preset as u8)
                        .await
                }
                .await;
                if let Err(error) = result {
                    show_error(weak, format!("EQ change failed: {error}"));
                }
            });
        });
    }

    {
        let connection = connection.clone();
        let weak = window.as_weak();
        window.on_set_sidetone(move |level| {
            let connection = connection.clone();
            let weak = weak.clone();
            tokio::spawn(async move {
                let result = async {
                    HeadsetProxy::new(&connection)
                        .await?
                        .set_sidetone(level as u8)
                        .await
                }
                .await;
                if let Err(error) = result {
                    show_error(weak, format!("Sidetone change failed: {error}"));
                }
            });
        });
    }

    {
        let connection = connection.clone();
        let weak = window.as_weak();
        window.on_set_thx(move |enabled| {
            let connection = connection.clone();
            let weak = weak.clone();
            tokio::spawn(async move {
                let result =
                    async { HeadsetProxy::new(&connection).await?.set_thx(enabled).await }.await;
                if let Err(error) = result {
                    show_error(weak, format!("THX change failed: {error}"));
                }
            });
        });
    }

    {
        let connection = connection.clone();
        let weak = window.as_weak();
        window.on_set_anc(move |enabled, level| {
            let connection = connection.clone();
            let weak = weak.clone();
            tokio::spawn(async move {
                let result = async {
                    HeadsetProxy::new(&connection)
                        .await?
                        .set_anc(enabled, level as u8)
                        .await
                }
                .await;
                if let Err(error) = result {
                    show_error(weak, format!("ANC change failed: {error}"));
                }
            });
        });
    }

    {
        let connection = connection.clone();
        let weak = window.as_weak();
        window.on_set_power_savings(move |minutes| {
            let connection = connection.clone();
            let weak = weak.clone();
            tokio::spawn(async move {
                let result = async {
                    HeadsetProxy::new(&connection)
                        .await?
                        .set_power_savings(minutes as u8)
                        .await
                }
                .await;
                if let Err(error) = result {
                    show_error(weak, format!("Power setting failed: {error}"));
                }
            });
        });
    }

    {
        use futures_util::StreamExt;

        let connection = connection.clone();
        let weak = window.as_weak();
        tokio::spawn(async move {
            let Ok(proxy) = HeadsetProxy::new(&connection).await else {
                return;
            };
            let Ok(mut battery) = proxy.receive_battery_changed().await else {
                return;
            };
            let mut connected = proxy.receive_connected_changed().await;
            let mut eq = proxy.receive_eq_preset_changed().await;
            let mut sidetone = proxy.receive_sidetone_changed().await;
            let mut thx = proxy.receive_thx_enabled_changed().await;
            let mut anc = proxy.receive_anc_enabled_changed().await;
            let mut anc_level = proxy.receive_anc_level_changed().await;
            let mut power = proxy.receive_power_savings_minutes_changed().await;

            loop {
                tokio::select! {
                    Some(signal) = battery.next() => {
                        if let Ok(args) = signal.args() {
                            let percentage = args.percentage as i32;
                            let charging = args.charging;
                            let weak = weak.clone();
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(window) = weak.upgrade() {
                                    window.set_battery_pct(percentage);
                                    window.set_charging(charging);
                                }
                            });
                        }
                    }
                    Some(change) = connected.next() => if let Ok(value) = change.get().await {
                        let weak = weak.clone();
                        let _ = slint::invoke_from_event_loop(move || if let Some(window) = weak.upgrade() { window.set_connected(value); });
                    },
                    Some(change) = eq.next() => if let Ok(value) = change.get().await {
                        let weak = weak.clone();
                        let _ = slint::invoke_from_event_loop(move || if let Some(window) = weak.upgrade() { window.set_eq_preset(value as i32); });
                    },
                    Some(change) = sidetone.next() => if let Ok(value) = change.get().await {
                        let weak = weak.clone();
                        let _ = slint::invoke_from_event_loop(move || if let Some(window) = weak.upgrade() { window.set_sidetone(value as i32); });
                    },
                    Some(change) = thx.next() => if let Ok(value) = change.get().await {
                        let weak = weak.clone();
                        let _ = slint::invoke_from_event_loop(move || if let Some(window) = weak.upgrade() { window.set_thx_enabled(value); });
                    },
                    Some(change) = anc.next() => if let Ok(value) = change.get().await {
                        let weak = weak.clone();
                        let _ = slint::invoke_from_event_loop(move || if let Some(window) = weak.upgrade() { window.set_anc_enabled(value); });
                    },
                    Some(change) = anc_level.next() => if let Ok(value) = change.get().await {
                        let weak = weak.clone();
                        let _ = slint::invoke_from_event_loop(move || if let Some(window) = weak.upgrade() { window.set_anc_level(value as i32); });
                    },
                    Some(change) = power.next() => if let Ok(value) = change.get().await {
                        let weak = weak.clone();
                        let _ = slint::invoke_from_event_loop(move || if let Some(window) = weak.upgrade() { window.set_power_savings(value as i32); });
                    },
                    else => break,
                }
            }
        });
    }

    window.run()?;
    Ok(())
}
