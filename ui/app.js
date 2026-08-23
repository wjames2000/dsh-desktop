// Tauri v2 IPC：@tauri-apps/api 的 invoke
// 注意：本应用没有 npm 依赖（纯 HTML/JS），用 window.__TAURI__.core.invoke
const { invoke } = window.__TAURI__.core;

let currentConfig = null;

async function refresh() {
  try {
    currentConfig = await invoke('get_config');
  } catch (e) {
    document.getElementById('save-status').textContent = '读取配置失败: ' + e;
    currentConfig = null;
  }
  if (!currentConfig) return;
  document.getElementById('workspace-value').textContent =
    currentConfig.workspace_dir || '未设置（首次启动时选择）';
  document.getElementById('datadir-value').textContent =
    currentConfig.data_dir || '默认 (~/.dsh)';
  document.getElementById('port-input').value = currentConfig.port;
  document.getElementById('autostart-check').checked = currentConfig.autostart;
  document.getElementById('tray-check').checked = currentConfig.minimize_to_tray;
  document.getElementById('check-update-check').checked = currentConfig.check_updates_on_start;
}

async function save(partial) {
  if (!currentConfig) {
    document.getElementById('save-status').textContent = '配置未就绪，请稍后重试';
    return;
  }
  const next = { ...currentConfig, ...partial };
  try {
    await invoke('set_config', { cfg: next });
    currentConfig = next;
    document.getElementById('save-status').textContent = '已保存（部分设置下次启动生效）';
  } catch (e) {
    document.getElementById('save-status').textContent = '保存失败: ' + e;
  }
}

document.getElementById('choose-workspace').addEventListener('click', async () => {
  const picked = await invoke('choose_workspace');
  if (picked) { await save({ workspace_dir: picked }); await refresh(); }
});

document.getElementById('choose-datadir').addEventListener('click', async () => {
  const picked = await invoke('choose_workspace');
  if (picked) { await save({ data_dir: picked }); await refresh(); }
});

document.getElementById('reset-datadir').addEventListener('click', async () => {
  await save({ data_dir: null }); await refresh();
});

document.getElementById('port-input').addEventListener('change', (e) => {
  const v = Number(e.target.value);
  save({ port: Number.isFinite(v) && v > 0 ? v : 0 });
});

document.getElementById('autostart-check').addEventListener('change', (e) => {
  save({ autostart: e.target.checked });
});

document.getElementById('tray-check').addEventListener('change', (e) => {
  save({ minimize_to_tray: e.target.checked });
});

document.getElementById('check-update-check').addEventListener('change', (e) => {
  save({ check_updates_on_start: e.target.checked });
});

document.getElementById('check-update-now').addEventListener('click', async () => {
  const status = document.getElementById('update-status');
  const btn = document.getElementById('check-update-now');
  // 检查期间禁用按钮，防止并发触发多次下载安装
  btn.disabled = true;
  status.textContent = '正在检查…';
  try {
    const result = await invoke('check_for_updates');
    status.textContent = result || '已是最新版本';
  } catch (e) {
    status.textContent = '检查失败: ' + e;
  } finally {
    btn.disabled = false;
  }
});

document.getElementById('restart-service').addEventListener('click', async () => {
  const status = document.getElementById('service-status');
  status.textContent = '正在重启服务…';
  try {
    const port = await invoke('restart_service');
    status.textContent = `服务已重启（端口 ${port}）`;
  } catch (e) {
    status.textContent = '重启失败: ' + e;
  }
});

document.getElementById('quit-app').addEventListener('click', async () => {
  try {
    await invoke('quit_app');
  } catch (e) {
    document.getElementById('save-status').textContent = '退出失败: ' + e;
  }
});

refresh();
