mod docker;

use std::sync::Mutex;
use tauri::Manager;

/// Shared application state.
pub struct AppState {
    /// Directory (under the OS cache dir) holding the bundled `preflight.R`,
    /// bind-mounted into the container at preflight time.
    pub script_dir: std::path::PathBuf,
    /// Name of the currently-running clock container, used for cancellation.
    pub run_container: Mutex<Option<String>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let script_dir = docker::init_preflight_script(app.handle())?;
            app.manage(AppState {
                script_dir,
                run_container: Mutex::new(None),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            docker::check_docker,
            docker::check_image,
            docker::pull_image,
            docker::preflight,
            docker::run_clocks,
            docker::cancel_run,
            docker::default_image,
        ])
        .run(tauri::generate_context!())
        .expect("error while running beta2clocks application");
}
