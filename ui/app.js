// Tauri v2 IPC：@tauri-apps/api 的 invoke
// 注意：本应用没有 npm 依赖（纯 HTML/JS），用 window.__TAURI__.core.invoke
const { invoke } = window.__TAURI__.core;

let currentConfig = null;

async function refresh() {
  currentConfig = await invoke('get_config');
  document.getElementById('workspace-value').textContent =
    currentConfig.workspace_dir || '未设置（首次启动时选择）';
  document.getElementById('datadir-value').textContent =
    currentConfig.data_dir || '默认 (~/.dsh)';
  document.getElementById('port-input').value = currentConfig.port;
  document.getElementById('autostart-check').checked = currentConfig.autostart;
  document.getElementById('tray-check').checked = currentConfig.minimize_to_tray;
}

async function save(partial) {
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

refresh();
