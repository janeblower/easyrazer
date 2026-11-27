#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![deny(unsafe_code)]

mod about;
mod app;
mod changes;
mod dev;
mod dialogs;
mod gamepad;
mod icons;
mod instance;
mod jobs;
mod keys;
mod lighting;
mod session;
mod settings;
mod shell;
mod status;
mod text;
mod theme;
mod tray;
mod updates;
mod widgets;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use easyrazer_backend::{Backend, autostart, device};

const ICON: &[u8] = include_bytes!("../assets/icon.png");

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| matches!(a.as_str(), "--gamepad-install" | "--gamepad-replace" | "--gamepad-uninstall" | "--gamepad-boot-on" | "--gamepad-boot-off")) {
        if let Err(e) = easyrazer_backend::gamepad::driver_helper(&args[0], &args[1..]) {
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
    let started = Arc::new(AtomicBool::new(false));
    let created = started.clone();
    // eframe panics, not errs, when OpenGL has no usable configuration.
    let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| eframe::run_native(
        "EasyRazer",
        options,
        Box::new(move |cc| {
            created.store(true, Ordering::Relaxed);
            theme::install(&cc.egui_ctx);
            let tray = tray::Tray::new(&icon, &backend.lang(), &cc.egui_ctx).map_err(|e| e.to_string())?;
            instance.on_activate(cc.egui_ctx.clone());
            backend.start(shown);
            Ok(Box::new(app::App::new(backend, events, tray, &cc.egui_ctx, shown)))
        }),
    )));
    shutdown.shutdown();
    let opened = started.load(Ordering::Relaxed);
    match run {
        Ok(Err(e)) if !opened => {
            startup_failed(&shutdown.lang(), &e.to_string());
            Err(e)
        }
        Ok(result) => result,
        Err(panic) if !opened => {
            let text = panic.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| panic.downcast_ref::<String>().cloned()).unwrap_or_default();
            startup_failed(&shutdown.lang(), &text);
            std::process::exit(1);
        }
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

/// Built without a console, the exe would otherwise end without a word.
fn startup_failed(lang: &str, error: &str) {
    use easyrazer_backend::{api, i18n};
    use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
    let mut text = i18n::tf(lang, "startup.failed", &[("error", error)]);
    // Windows on ARM has OpenGL only through a Store package.
    let arm = cfg!(target_arch = "aarch64");
    if arm {
        text = format!("{text}\n\n{}", i18n::t(lang, "startup.glPack"));
    }
    let dialog = MessageDialog::new().set_level(MessageLevel::Error).set_title("EasyRazer").set_description(text);
    let buttons = if arm { MessageButtons::YesNo } else { MessageButtons::Ok };
    if dialog.set_buttons(buttons).show() == MessageDialogResult::Yes {
        let _ = api::open_link(api::Link::GlCompatibilityPack);
    }
}
