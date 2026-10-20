//! dsh Web UI 的认证引导。
//!
//! dsh >= 0.1.2 用进程级 token 保护 web 服务：启动时打印
//! `dsh web: http://127.0.0.1:PORT/?token=XXX`。访问该 URL 会 303 +
//! `Set-Cookie: dsh-auth-<authority>=<signed>` 后跳转干净首页；之后的页面与
//! `/api/*` 请求都靠这个 cookie 认证（cookie 名由请求 authority 派生、值由持久
//! signing secret 签名）。
//!
//! 策略（按可靠性排序，逐级降级）：
//! 1. Rust 侧完成 token→cookie 交换，用 WebView 原生 `set_cookie` 注入，并轮询
//!    `cookies_for_url` 确认 cookie 真的进了 cookie jar；
//! 2. 无论是否进 jar，导航后都用 `eval` 写一次 `document.cookie` 并 reload
//!    （JS 同步生效，不依赖 WebView 的 Set-Cookie 处理与异步 set_cookie）；
//! 3. 取 cookie 失败时回退为直接导航带 token 的 URL，由 WebView 自行交换。

use std::time::Duration;
use tauri::webview::Cookie;
use tauri::{AppHandle, Manager, Url};

/// 从 `Set-Cookie` 头里取出 `name=value` 对（可能多条）。
pub fn fetch_auth_cookies(auth_url: &str) -> Result<Vec<(String, String)>, String> {
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("构建 HTTP 客户端失败: {e}"))?;
    let resp = client
        .get(auth_url)
        .send()
        .map_err(|e| format!("访问认证 URL 失败: {e}"))?;

    let mut pairs = Vec::new();
    for raw in resp.headers().get_all(reqwest::header::SET_COOKIE) {
        let Ok(text) = raw.to_str() else { continue };
        let Some(head) = text.split(';').next() else { continue };
        let Some((name, value)) = head.split_once('=') else { continue };
        let (name, value) = (name.trim(), value.trim());
        if !name.is_empty() && !value.is_empty() {
            pairs.push((name.to_string(), value.to_string()));
        }
    }
    if pairs.is_empty() {
        return Err("token 交换未返回 Set-Cookie（服务可能未启用认证或版本不匹配）".to_string());
    }
    Ok(pairs)
}

/// 认证 URL 对应的干净首页 URL（去掉 token 查询参数）。
pub fn clean_url(auth_url: &str) -> Option<String> {
    let mut url = Url::parse(auth_url).ok()?;
    url.set_query(None);
    Some(url.to_string())
}

/// 日志用：只保留首尾，避免整段 token 落日志。
fn mask(value: &str) -> String {
    let n = value.chars().count();
    if n <= 10 {
        return "***".to_string();
    }
    let head: String = value.chars().take(6).collect();
    let tail: String = value.chars().skip(n - 4).collect();
    format!("{head}…{tail}")
}

/// 认证引导（主入口）。详见模块文档。
pub fn bootstrap(app: &AppHandle, auth_url: &str) -> Result<(), String> {
    let win = app
        .get_webview_window("main")
        .ok_or_else(|| "主窗口不存在".to_string())?;

    // ── 1. token→cookie 交换 ──────────────────────────────────────────
    let pairs = match fetch_auth_cookies(auth_url) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[auth] 取 cookie 失败，回退 WebView 自行交换: {e}");
            if let Ok(u) = auth_url.parse() {
                let _ = win.navigate(u);
            }
            return Ok(());
        }
    };
    let names: Vec<String> = pairs.iter().map(|(n, _)| n.clone()).collect();
    eprintln!(
        "[auth] 取得 {} 个 cookie: {:?}（值 {}）",
        pairs.len(),
        names,
        pairs.first().map(|(_, v)| mask(v)).unwrap_or_default()
    );

    let origin = Url::parse(auth_url).map_err(|e| format!("解析认证 URL 失败: {e}"))?;
    let host = origin.host_str().unwrap_or("127.0.0.1").to_string();
    let clean = clean_url(auth_url).unwrap_or_else(|| auth_url.to_string());

    // ── 2. 原生 set_cookie 注入并验证是否进入 cookie jar ──────────────
    let mut in_jar = false;
    for (name, value) in &pairs {
        let cookie = Cookie::build((name.clone(), value.clone()))
            .domain(host.clone())
            .path("/")
            .build();
        if let Err(e) = win.set_cookie(cookie) {
            eprintln!("[auth] set_cookie 失败（{name}）: {e}");
        }
    }
    if let Ok(clean_parsed) = clean.parse::<Url>() {
        for i in 0..25 {
            match win.cookies_for_url(clean_parsed.clone()) {
                Ok(cs) if !cs.is_empty() => {
                    in_jar = true;
                    eprintln!(
                        "[auth] cookie jar 确认 {} 个 cookie（第 {} 次探测）",
                        cs.len(),
                        i + 1
                    );
                    break;
                }
                Ok(_) => {}
                Err(e) => {
                    eprintln!("[auth] cookies_for_url 失败: {e}");
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    if !in_jar {
        eprintln!("[auth] set_cookie 未进入 cookie jar（将由 JS 注入兜底）");
    }

    // ── 3. 导航到干净首页 ────────────────────────────────────────────
    if let Ok(u) = clean.parse() {
        let _ = win.navigate(u);
    }
    // 轮询当前 URL，确认导航已落到目标 origin（否则 eval 会注入到旧 origin）
    let mut navigated = false;
    for _ in 0..60 {
        if let Ok(current) = win.url() {
            if current.host_str() == Some(host.as_str()) {
                navigated = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let current_url = win.url().map(|u| u.to_string()).unwrap_or_default();
    eprintln!("[auth] 导航完成={navigated}，当前 URL={current_url}");
    // 给页面渲染一点时间（401 页也需要建立 JS 上下文）
    std::thread::sleep(Duration::from_millis(600));

    // ── 4. JS 注入（同步生效）+ reload ───────────────────────────────
    let js = pairs
        .iter()
        .map(|(name, value)| {
            let literal =
                serde_json::to_string(&format!("{name}={value}; path=/; SameSite=Strict"))
                    .unwrap_or_else(|_| "\"\"".to_string());
            format!("document.cookie = {literal};")
        })
        .collect::<Vec<_>>()
        .join(" ");
    let script = format!("{js} location.reload();");
    match win.eval(&script) {
        Ok(()) => eprintln!("[auth] 已通过 JS 注入 cookie 并 reload（in_jar={in_jar}）"),
        Err(e) => eprintln!("[auth] eval 注入失败: {e}"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_url_strips_token() {
        assert_eq!(
            clean_url("http://127.0.0.1:3182/?token=abc").as_deref(),
            Some("http://127.0.0.1:3182/")
        );
    }

    #[test]
    fn clean_url_keeps_plain_url() {
        assert_eq!(
            clean_url("http://127.0.0.1:3080").as_deref(),
            Some("http://127.0.0.1:3080/")
        );
    }

    #[test]
    fn clean_url_rejects_invalid() {
        assert_eq!(clean_url("not a url"), None);
    }

    #[test]
    fn set_cookie_head_parsing() {
        let raw =
            "dsh-auth-abc=v1.payload.sig; Max-Age=2592000; Path=/; HttpOnly; SameSite=Strict";
        let (name, value) = raw.split(';').next().unwrap().split_once('=').unwrap();
        assert_eq!(name, "dsh-auth-abc");
        assert_eq!(value, "v1.payload.sig");
    }

    #[test]
    fn mask_hides_middle() {
        let m = mask("v1.abcdefghijklmnop");
        assert!(m.starts_with("v1.abc"));
        assert!(m.contains('…'));
    }
}
