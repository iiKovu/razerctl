#[test]
fn gui_contains_only_portable_headset_controls() {
    let rust = include_str!("../src/main.rs");
    let ui = include_str!("../ui/main.slint");

    for forbidden in [
        "systemctl",
        "journalctl",
        "mod pipewire",
        "Routing",
        "game-chat",
    ] {
        assert!(
            !rust.contains(forbidden) && !ui.contains(forbidden),
            "portable GUI still contains {forbidden} integration"
        );
    }

    for control in [
        "set-eq",
        "set-sidetone",
        "set-thx",
        "set-anc",
        "set-power-savings",
    ] {
        assert!(ui.contains(control), "GUI is missing {control}");
    }

    assert!(!ui.contains("changed(value)"));
}

#[test]
fn gui_uses_the_desktop_palette_and_custom_controls() {
    let ui = include_str!("../ui/main.slint");

    for color in [
        "#0D0D0F", "#EBE1D5", "#FE88C2", "#38383E", "#5A585E", "#C05555",
    ] {
        assert!(ui.contains(color), "GUI palette is missing {color}");
    }

    for custom_control in [
        "component SegmentButton",
        "component ToggleSwitch",
        "component SteppedSlider",
    ] {
        assert!(
            ui.contains(custom_control),
            "GUI is missing {custom_control}"
        );
    }

    assert!(!ui.contains("std-widgets.slint"));
}

#[test]
fn gui_is_resizable_instead_of_publishing_fixed_size_hints() {
    let ui = include_str!("../ui/main.slint");

    assert!(ui.contains("preferred-width:"));
    assert!(ui.contains("preferred-height:"));
    assert!(!ui.contains("width: 600px;"));
    assert!(!ui.contains("height: 720px;"));
}

#[test]
fn stepped_slider_previews_locally_and_commits_once_on_release() {
    let ui = include_str!("../ui/main.slint");

    for behavior in [
        "property <int> preview-value",
        "callback committed(int)",
        "PointerEventKind.up",
        "key-pressed(event)",
        "key-released(event)",
    ] {
        assert!(
            ui.contains(behavior),
            "stepped slider is missing {behavior}"
        );
    }

    assert_eq!(
        ui.matches("root.committed(root.preview-value)").count(),
        2,
        "slider must commit once from pointer release and once from key release"
    );
}

#[test]
fn stepped_slider_fill_is_anchored_to_the_track_origin() {
    let ui = include_str!("../ui/main.slint");

    assert!(
        ui.contains("slider-fill := Rectangle {\n            x: 0px;"),
        "slider fill must start at the track origin instead of being centered"
    );
}
