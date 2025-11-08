//! 测试工具模块
//!
//! 提供测试相关的工具函数和辅助功能

#[cfg(test)]
pub mod utils {
    use std::path::Path;
    use tokio::sync::OnceCell;

    // Initialize the test database once globally
    static INIT: OnceCell<()> = OnceCell::const_new();

    // Setup test database
    pub async fn setup_test_db() -> anyhow::Result<()> {
        INIT.get_or_init(|| async {
            let db_path = std::env::temp_dir().join("test_rusqlite_global.db");
            if db_path.exists() {
                std::fs::remove_file(&db_path).expect("Failed to remove existing test DB");
            }

            let db_path_str = db_path
                .to_str()
                .expect("Failed to convert DB path to string");
            crate::storage::db::set_db_path(Path::new(db_path_str))
                .await
                .expect("Failed to initialize test database");
        })
        .await;
        Ok(())
    }
}
