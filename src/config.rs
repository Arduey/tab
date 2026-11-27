//! 运行配置。所有值都可被环境变量覆盖，安装向导不需要写配置文件：
//! 站点标题、管理员账号都存在 SQLite 里，数据目录默认就是进程工作目录下的 data/。

use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    /// 数据目录（数据库、图标缓存、上传图标）
    pub data_dir: PathBuf,
    pub bind: String,
    pub port: u16,
}

impl Config {
    pub fn from_env() -> Self {
        let data_dir = std::env::var("TAB_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("data"));
        let bind = std::env::var("TAB_BIND").unwrap_or_else(|_| "0.0.0.0".into());
        let port = std::env::var("TAB_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(8080);
        Self {
            data_dir,
            bind,
            port,
        }
    }

    pub fn icons_dir(&self) -> PathBuf {
        self.data_dir.join("icons")
    }

    pub fn uploads_dir(&self) -> PathBuf {
        self.data_dir.join("uploads")
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("tab.sqlite3")
    }

    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.data_dir)?;
        std::fs::create_dir_all(self.icons_dir())?;
        std::fs::create_dir_all(self.uploads_dir())?;
        Ok(())
    }
}