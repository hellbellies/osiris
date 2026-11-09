use tauri::Manager;

mod commands;
mod db;
mod error;
mod models;
mod services;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let conn = db::open(&app.path().app_data_dir()?)?;
            app.manage(db::Db::new(conn));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::jira::get_jira_settings,
            commands::jira::update_jira_settings,
            commands::jira::clear_jira_token,
            commands::jira::test_jira_connection,
            commands::jira::list_jira_issues,
            commands::jira::sync_jira_issues,
            commands::jira::open_jira_issue,
            commands::jira::get_jira_status_filter,
            commands::jira::set_jira_status_filter,
            commands::github::get_github_settings,
            commands::github::update_github_settings,
            commands::github::clear_github_token,
            commands::github::test_github_connection,
            commands::github::list_pull_requests,
            commands::github::list_jira_issue_pull_requests,
            commands::github::sync_pull_requests,
            commands::github::refresh_pull_request,
            commands::github::open_pull_request,
            commands::todos::list_todos,
            commands::todos::create_todo,
            commands::todos::update_todo,
            commands::todos::set_todo_completed,
            commands::todos::delete_todo,
            commands::todos::open_todo_link,
            commands::workstreams::list_workstreams,
            commands::workstreams::get_workstream,
            commands::workstreams::create_workstream,
            commands::workstreams::rename_workstream,
            commands::workstreams::set_workstream_completed,
            commands::workstreams::delete_workstream,
            commands::workstreams::set_item_workstream,
            commands::workstreams::search_workstream_items,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
