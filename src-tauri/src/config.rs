use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// 工作目录（workspace）
    pub workspace_dir: Option<String>,
    /// 数据目录（DSH_HOME）；None 表示使用默认 ~/.dsh
    pub data_dir: Option<String>,
    /// 端口偏好，0 表示让 OS 分配
    pub port: u16,
    /// 开机自启
    pub autostart: bool,
    /// 关窗行为：true=最小化到托盘，false=直接退出
    pub minimize_to_tray: bool,
    /// 更新通道
    pub update_channel: String,
    /// 启动时自动检查更新
    pub check_updates_on_start: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            workspace_dir: None,
            data_dir: None,
            port: 3080,
            autostart: false,
            minimize_to_tray: true,
            update_channel: "stable".to_string(),
            check_updates_on_start: true,
        }
    }
}

impl AppConfig {
    /// 配置文件路径：系统配置目录/dsh-desktop/app-config.json
    pub fn config_path() -> PathBuf {
        let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        base.join("dsh-desktop").join("app-config.json")
    }

    pub fn load() -> Self {
        Self::load_from(&Self::config_path())
    }

    /// 从指定路径读取配置：读失败或解析失败时回退默认配置
    fn load_from(path: &Path) -> Self {
        match fs::read_to_string(path) {
            Ok(s) => match serde_json::from_str(&s) {
                Ok(cfg) => cfg,
                Err(e) => {
                    eprintln!("[dsh-desktop] 配置文件解析失败（{path:?}），使用默认配置: {e}");
                    Self::default()
                }
            },
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        self.save_to(&Self::config_path())
    }

    /// 原子写：先写临时文件再 rename
    fn save_to(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let tmp = path.with_extension("json.tmp");
        let data = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(&tmp, data).map_err(|e| e.to_string())?;
        fs::rename(&tmp, path).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_sane() {
        let c = AppConfig::default();
        assert_eq!(c.port, 3080);
        assert!(c.minimize_to_tray);
        assert!(c.check_updates_on_start);
    }

    #[test]
    fn save_and_load_roundtrip() {
        let mut c = AppConfig::default();
        c.workspace_dir = Some("/tmp/ws".to_string());
        c.data_dir = Some("/tmp/dsh".to_string());
        c.port = 4000;
        c.autostart = true;
        c.minimize_to_tray = false;
        c.update_channel = "beta".to_string();
        c.check_updates_on_start = false;

        let dir = std::env::temp_dir().join("dsh-desktop-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("app-config.json");

        c.save_to(&path).unwrap();
        let loaded = AppConfig::load_from(&path);
        assert_eq!(loaded, c);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_missing_file_returns_default() {
        let dir = std::env::temp_dir().join("dsh-desktop-missing-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("does-not-exist.json");
        let loaded = AppConfig::load_from(&path);
        assert_eq!(loaded, AppConfig::default());
        std::fs::remove_dir_all(&dir).ok();
    }
}
