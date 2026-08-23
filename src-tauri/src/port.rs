use std::net::TcpListener;

/// 从 preferred 开始找第一个可绑定的端口；preferred=0 时由 OS 分配。
/// 返回 (实际端口)。范围限制 1024..=65535，最多尝试 100 个。
pub fn find_free_port(preferred: u16) -> u16 {
    if preferred == 0 {
        return TcpListener::bind("127.0.0.1:0")
            .map(|l| l.local_addr().unwrap().port())
            .unwrap_or(3080);
    }
    let start = preferred.max(1024);
    for port in start..=(start.saturating_add(100).min(65535)) {
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return port;
        }
    }
    // 全部占用则让 OS 分配
    TcpListener::bind("127.0.0.1:0")
        .map(|l| l.local_addr().unwrap().port())
        .unwrap_or(3080)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_free_port_returns_bindable() {
        let p = find_free_port(3080);
        assert!(p >= 1024 && p <= 65535);
        // 端口应该真的是空闲的（能绑定）
        assert!(TcpListener::bind(("127.0.0.1", p)).is_ok());
    }

    #[test]
    fn zero_means_os_assigns() {
        let p = find_free_port(0);
        assert!(p >= 1024 && p <= 65535);
    }

    #[test]
    fn skips_occupied_port() {
        let occupied = TcpListener::bind("127.0.0.1:0").unwrap();
        let occupied_port = occupied.local_addr().unwrap().port();
        let p = find_free_port(occupied_port);
        assert_ne!(p, occupied_port);
    }
}
