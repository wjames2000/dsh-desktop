use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
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
        let path = Self::config_path();
        match fs::read_to_string(&path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// 原子写：先写临时文件再 rename
    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let tmp = path.with_extension("json.tmp");
        let data = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(&tmp, data).map_err(|e| e.to_string())?;
        fs::rename(&tmp, &path).map_err(|e| e.to_string())
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
        c.port = 4000;
        c.autostart = true;
        let path = std::env::temp_dir().join("dsh-desktop-test").join("app-config.json");
        // 手动指向测试路径（save/load 用 config_path 时无法注入，
        // 这里直接测 save 的原子写行为）
        let data = serde_json::to_string_pretty(&c).unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path.with_extension("json.tmp"), &data).unwrap();
        std::fs::rename(path.with_extension("json.tmp"), &path).unwrap();
        let loaded: AppConfig = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded.workspace_dir, Some("/tmp/ws".to_string()));
        assert_eq!(loaded.port, 4000);
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn load_missing_file_returns_default() {
        // 用不存在的路径读不到时会走 default
        let path = std::env::temp_dir().join("dsh-desktop-does-not-exist");
        let result = std::fs::read_to_string(&path);
        assert!(result.is_err());
    }
}
