use search::{SearchResult, SearchError};

#[tauri::command]
fn search_files(query: String, max: Option<u32>) -> Result<Vec<SearchResult>, String> {
    search::search(&query, max.unwrap_or(200))
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![search_files])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
