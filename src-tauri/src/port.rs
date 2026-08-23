use std::net::TcpListener;

/// 从 preferred 开始找第一个可绑定的端口；preferred=0 时由 OS 分配。
/// 返回实际端口。
///
/// 注意（TOCTOU）：返回的端口不保证被保留——探测用的 listener 在探测
/// 完成后立即释放，调用方真正 bind 之前端口可能被其他进程抢占。
/// 调用方 bind 失败时应重新探测或重试。
pub fn find_free_port(preferred: u16) -> u16 {
    if preferred == 0 {
        return os_assigned_port();
    }
    let start = preferred.max(1024);
    // 最多尝试 101 个（preferred 及其后 100 个）
    for port in start..=(start.saturating_add(100).min(65535)) {
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return port;
        }
    }
    // 全部占用则让 OS 分配
    os_assigned_port()
}

/// 让 OS 分配一个空闲端口；失败时回退到 FALLBACK_PORT。
fn os_assigned_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .unwrap_or(FALLBACK_PORT)
}

const FALLBACK_PORT: u16 = 3080;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_free_port_returns_bindable() {
        // 从 20000 起探测：与 fallback 测试的 30000-30100 区间隔离，避免并行运行竞争
        let p = find_free_port(20000);
        // 上限 65535 由 u16 类型保证
        assert!(p >= 1024);
        // 端口应该真的是空闲的（能绑定）
        assert!(TcpListener::bind(("127.0.0.1", p)).is_ok());
    }

    #[test]
    fn zero_means_os_assigns() {
        let p = find_free_port(0);
        // 上限 65535 由 u16 类型保证
        assert!(p >= 1024);
    }

    #[test]
    fn skips_occupied_port() {
        let occupied = TcpListener::bind("127.0.0.1:0").unwrap();
        let occupied_port = occupied.local_addr().unwrap().port();
        let p = find_free_port(occupied_port);
        assert_ne!(p, occupied_port);
    }

    #[test]
    fn fallback_to_os_when_all_occupied() {
        // 占用 [30000, 30100] 共 101 个端口，独立区间避免与其他测试竞争；
        // 覆盖"最多尝试 101 个"边界与兜底分支
        let mut listeners = Vec::new();
        for port in 30000..=30100 {
            match TcpListener::bind(("127.0.0.1", port)) {
                Ok(l) => listeners.push(l),
                Err(_) => {} // 该端口已被占用（外部进程），也算"不可用"
            }
        }
        let p = find_free_port(30000);
        // 上限 65535 由 u16 类型保证
        assert!(p >= 1024);
        // 结果不应等于任何被我们成功占用的端口（find_free_port 会跳过它们）
        let occupied_ports: Vec<u16> = listeners
            .iter()
            .map(|l| l.local_addr().unwrap().port())
            .collect();
        assert!(!occupied_ports.contains(&p));
    }
}
