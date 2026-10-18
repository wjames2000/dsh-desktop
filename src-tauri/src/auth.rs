//! dsh Web UI 的认证引导。
//!
//! dsh >= 0.1.2 用进程级 token 保护 web 服务：启动时打印
//! `dsh web: http://127.0.0.1:PORT/?token=XXX`，浏览器访问该 URL 会 303 +
//! `Set-Cookie` 后跳转干净首页；之后的 `/api/*` 请求靠这个 cookie 认证。
//!
//! WebView 自己走「导航带 token 的 URL → 跟随 303 → 保存 cookie」本应可行，
//! 但实测部分环境（WKWebView 对无 domain 的 Set-Cookie、或重定向时序）会丢 cookie，
//! 页面就停在 `dsh web authentication required`。因此这里在 Rust 侧先做一次
//! token 交换、把 cookie 显式注入 WebView，再导航到干净 URL——不依赖 WebView 的
//! cookie 保存行为。

use std::time::Duration;
use tauri::webview::Cookie;
use tauri::{AppHandle, Manager, Url};

/// 用认证 URL 完成 token→cookie 交换，并把 cookie 注入主窗口 WebView。
///
/// 返回注入的 cookie 数量（>=1 表示成功）。
pub fn bootstrap_auth_cookie(app: &AppHandle, auth_url: &str) -> Result<usize, String> {
    let url = Url::parse(auth_url).map_err(|e| format!("认证 URL 解析失败: {e}"))?;
    let host = url.host_str().unwrap_or("127.0.0.1").to_string();

    // 1) 不跟随重定向：303 响应里才有 Set-Cookie
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("构建 HTTP 客户端失败: {e}"))?;
    let resp = client
        .get(auth_url)
        .send()
        .map_err(|e| format!("访问认证 URL 失败: {e}"))?;

    // 2) 解析所有 Set-Cookie
    let mut cookies = Vec::new();
    for raw in resp.headers().get_all(reqwest::header::SET_COOKIE) {
        let Ok(text) = raw.to_str() else { continue };
        let Ok(parsed) = Cookie::parse(text.to_string()) else { continue };
        // Set-Cookie 通常不带 Domain（host-only）：显式补上，确保注入到正确主机
        let cookie = if parsed.domain().is_some() {
            parsed.into_owned()
        } else {
            let mut c = parsed.into_owned();
            c.set_domain(host.clone());
            c
        };
        cookies.push(cookie);
    }
    if cookies.is_empty() {
        return Err("token 交换未返回 Set-Cookie（服务可能未启用认证或版本不匹配）".to_string());
    }

    // 3) 注入主窗口 WebView
    let win = app
        .get_webview_window("main")
        .ok_or_else(|| "主窗口不存在，无法注入认证 cookie".to_string())?;
    let mut injected = 0usize;
    for cookie in cookies {
        match win.set_cookie(cookie) {
            Ok(()) => injected += 1,
            Err(e) => eprintln!("[auth] 注入 cookie 失败: {e}"),
        }
    }
    if injected == 0 {
        return Err("认证 cookie 注入失败".to_string());
    }
    Ok(injected)
}

/// 认证 URL 对应的干净首页 URL（去掉 token 查询参数）。
pub fn clean_url(auth_url: &str) -> Option<String> {
    let mut url = Url::parse(auth_url).ok()?;
    url.set_query(None);
    Some(url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_url_strips_token() {
        let auth = "http://127.0.0.1:3182/?token=abc";
        assert_eq!(clean_url(auth).as_deref(), Some("http://127.0.0.1:3182/"));
    }

    #[test]
    fn clean_url_keeps_plain_url() {
        let plain = "http://127.0.0.1:3080";
        assert_eq!(clean_url(plain).as_deref(), Some("http://127.0.0.1:3080/"));
    }

    #[test]
    fn clean_url_rejects_invalid() {
        assert_eq!(clean_url("not a url"), None);
    }
}
