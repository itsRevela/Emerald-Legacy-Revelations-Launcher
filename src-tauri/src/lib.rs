use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use tauri::{AppHandle, Emitter, State, Manager};
use futures_util::StreamExt;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tauri_plugin_opener::OpenerExt; 

pub struct DownloadState { pub token: Arc<Mutex<Option<CancellationToken>>> }

fn get_app_dir(app: &AppHandle) -> PathBuf {
    app.path().app_local_data_dir().unwrap_or_else(|_| {
        std::env::current_dir().unwrap_or_default()
    })
}

#[tauri::command]
fn save_config(app: AppHandle, username: String) {
    let path = get_app_dir(&app).join("emerald_legacy_config.txt");
    let _ = fs::create_dir_all(path.parent().unwrap());
    let _ = fs::write(path, &username);
}

#[tauri::command]
fn load_config(app: AppHandle) -> String {
    let path = get_app_dir(&app).join("emerald_legacy_config.txt");
    fs::read_to_string(path).unwrap_or_default()
}

#[tauri::command]
#[allow(non_snake_case)]
fn check_game_installed(app: AppHandle, instanceId: String) -> bool {
    get_app_dir(&app).join("instances").join(&instanceId).join("Minecraft.Client.exe").exists()
}

#[tauri::command]
#[allow(non_snake_case)]
fn open_instance_folder(app: AppHandle, instanceId: String) {
    let dir = get_app_dir(&app).join("instances").join(&instanceId);
    if dir.exists() {
        let _ = app.opener().open_path(dir.to_str().unwrap(), None::<&str>);
    }
}

#[tauri::command]
async fn cancel_download(state: State<'_, DownloadState>) -> Result<(), String> {
    if let Some(token) = state.token.lock().await.take() { token.cancel(); }
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
async fn download_and_install(app: AppHandle, state: State<'_, DownloadState>, url: String, instanceId: String) -> Result<String, String> {
    let root = get_app_dir(&app);
    let instance_dir = root.join("instances").join(&instanceId);
    let token = CancellationToken::new();
    let child_token = token.clone();
    { *state.token.lock().await = Some(token); }
    
    if instance_dir.exists() { let _ = fs::remove_dir_all(&instance_dir); }
    fs::create_dir_all(&instance_dir).map_err(|e| e.to_string())?;
    
    let zip_path = root.join(format!("temp_{}.zip", instanceId));
    let response = reqwest::get(&url).await.map_err(|e| e.to_string())?;
    
    if !response.status().is_success() {
        return Err(format!("Download failed: {}", response.status()));
    }

    let total_size = response.content_length().unwrap_or(0) as f64;
    let mut file = fs::File::create(&zip_path).map_err(|e| e.to_string())?;
    let mut downloaded = 0.0;
    let mut stream = response.bytes_stream();
    
    while let Some(chunk) = stream.next().await {
        if child_token.is_cancelled() {
            drop(file); let _ = fs::remove_file(&zip_path);
            return Err("CANCELLED".into());
        }
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as f64;
        if total_size > 0.0 { let _ = app.emit("download-progress", downloaded / total_size * 100.0); }
    }
    
    drop(file);
    { *state.token.lock().await = None; }
    
    let status = Command::new("tar")
        .args(["-xf", zip_path.to_str().unwrap(), "-C", instance_dir.to_str().unwrap()])
        .status()
        .map_err(|e| e.to_string())?;
        
    let _ = fs::remove_file(&zip_path);

    if !status.success() {
        return Err("Extraction failed".into());
    }

    if let Ok(entries) = fs::read_dir(&instance_dir) {
        let entries_vec: Vec<_> = entries.flatten().collect();
        
        if entries_vec.len() == 1 && entries_vec[0].path().is_dir() {
            let inner_dir = entries_vec[0].path();
            
            if let Ok(inner_entries) = fs::read_dir(&inner_dir) {
                for inner_entry in inner_entries.flatten() {
                    let file_name = inner_entry.file_name();
                    let dest_path = instance_dir.join(file_name);
                    let _ = fs::rename(inner_entry.path(), dest_path);
                }
            }
            let _ = fs::remove_dir(&inner_dir);
        }
    }
    
    Ok("Success".into())
}

#[tauri::command]
#[allow(non_snake_case)]
async fn launch_game(app: AppHandle, instanceId: String) -> Result<(), String> {
    let root = get_app_dir(&app);
    let instance_dir = root.join("instances").join(&instanceId);
    let config_path = root.join("emerald_legacy_config.txt");
    let username = fs::read_to_string(config_path).unwrap_or_else(|_| "Player".into());
    
    let _ = fs::write(instance_dir.join("username.txt"), &username);
    
    let game_exe = instance_dir.join("Minecraft.Client.exe");
    if !game_exe.exists() {
        return Err("Game executable not found in instance folder.".into());
    }

    let _ = Command::new(&game_exe).spawn().map_err(|e| e.to_string())?;

    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .manage(DownloadState { token: Arc::new(Mutex::new(None)) })
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![launch_game, check_game_installed, save_config, load_config, download_and_install, open_instance_folder, cancel_download])
        .run(tauri::generate_context!())
        .expect("error");
}