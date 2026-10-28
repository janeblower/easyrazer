#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![deny(unsafe_code)]

mod app;
mod icons;
mod instance;
mod status;
mod theme;
mod tray;

use easyrazer_backend::{Backend, autostart, device, gamepad};

const ICON: &[u8] = include_bytes!("../../../app/src-tauri/icons/128x128.png");

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| matches!(a.as_str(), "--gamepad-install" | "--gamepad-replace" | "--gamepad-uninstall" | "--gamepad-boot-on" | "--gamepad-boot-off")) {
        if let Err(e) = gamepad::driver_helper(&args[0], &args[1..]) {
            eprintln!("hidshim: {e}");
            std::process::exit(1);
        }
        return Ok(());
    }
    let Some(instance) = instance::acquire() else { return Ok(()) };
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Only the main thread takes the process down; elsewhere panics are caught and the engine restarts.
        if std::thread::current().name() == Some("main") {
            device::release_keyboard();
        }
        default_hook(info);
    }));
    let (backend, events) = Backend::new().expect("hidapi init");
    let shown = !args.iter().any(|a| a == autostart::TRAY_ARG);
    let icon = eframe::icon_data::from_png_bytes(ICON).expect("bundled icon");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("EasyRazer")
            .with_inner_size([1153.0, 923.0])
            .with_min_inner_size([1153.0, 923.0])
            .with_icon(icon.clone())
            .with_visible(shown),
        ..Default::default()
    };
    let shutdown = backend.clone();
    let result = eframe::run_native(
        "EasyRazer",
        options,
        Box::new(move |cc| {
            theme::install(&cc.egui_ctx);
            let tray = tray::Tray::new(&icon, &backend.lang(), &cc.egui_ctx).map_err(|e| e.to_string())?;
            instance.on_activate(cc.egui_ctx.clone());
            backend.start(shown);
            Ok(Box::new(app::App::new(backend, events, tray, &cc.egui_ctx, shown)))
        }),
    );
    shutdown.shutdown();
    result
}
