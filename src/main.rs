#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod backend;
mod ui;

/// Smallest window the layout is designed for (points).
pub const MIN_WINDOW: [f32; 2] = [980.0, 620.0];

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version") {
        attach_parent_console();
        println!("{} {}", backend::APP_NAME, backend::update::CURRENT_VERSION);
        return Ok(());
    }
    if args.iter().any(|a| a == "--demo") {
        std::env::set_var("AKULAV_DEMO", "1");
    }
    let startup = match backend::install::on_startup(&args) {
        backend::install::Startup::Exit => return Ok(()),
        backend::install::Startup::Run(info) => info,
    };

    let icon = image::load_from_memory(include_bytes!("../assets/icon.png")).ok().map(|img| {
        let img = img.to_rgba8();
        egui::IconData { width: img.width(), height: img.height(), rgba: img.into_raw() }
    });

    let mut viewport = egui::ViewportBuilder::default()
        .with_title(backend::APP_NAME)
        .with_app_id(backend::APP_ID)
        .with_inner_size([1280.0, 800.0])
        .with_min_inner_size(MIN_WINDOW)
        .with_decorations(false)
        .with_resizable(true);
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }

    // Window size/position live in the app's own folder, not in %APPDATA% / ~/.local/share.
    let persistence_path = Some(backend::install::data_root().join(backend::install::WINDOW_STATE_FILE));
    let options = eframe::NativeOptions { viewport, centered: true, persistence_path, ..Default::default() };
    eframe::run_native(backend::APP_ID, options, Box::new(move |cc| Ok(Box::new(ui::app::App::new(cc, startup)))))
}

/// Release builds use the GUI subsystem; reattach to the terminal so `--version` prints.
#[cfg(windows)]
fn attach_parent_console() {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn AttachConsole(pid: u32) -> i32;
    }
    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
    // SAFETY: plain Win32 call with a constant argument.
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

#[cfg(not(windows))]
fn attach_parent_console() {}
