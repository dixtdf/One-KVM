'use strict';
let token = '';
let currentUser = null;
const consoles = new Set();
async function closeConsoles() { const current = [...consoles]; consoles.clear(); await Promise.all(current.map(c => c.close())); }
const $ = id => document.getElementById(id);
const notice = (message, error = false) => { $('notice').textContent = message; $('notice').classList.toggle('error', error); };
async function api(path, method = 'GET', body) {
  const response = await fetch(path, { method, credentials: 'omit', cache: 'no-store',
    headers: { ...(token ? { Authorization: `Bearer ${token}` } : {}), ...(body ? { 'Content-Type': 'application/json' } : {}) },
    body: body ? JSON.stringify(body) : undefined });
  const result = await response.json();
  if (!response.ok) {
    if (response.status === 401 && token) reset();
    throw new Error(result.error || '请求未成功');
  }
  return result;
}
function reset() {
  closeConsoles();
  token = ''; currentUser = null; $('workspace').hidden = true; $('administration').hidden = true;
  $('login').hidden = false; $('logout').hidden = true; $('identity').textContent = '请登录管理服务';
  $('channels').replaceChildren();
  for (const form of document.querySelectorAll('form')) form.reset();
}
function options(select, items, empty) {
  select.replaceChildren();
  if (empty !== undefined) { const item = new Option(empty, ''); select.add(item); }
  for (const item of items) select.add(new Option(item.name || item.label || item.username, item.id));
}
async function refresh() {
  await closeConsoles();
  const channels = await api('/channels');
  $('channels').replaceChildren();
  for (const c of channels) {
    const card = document.createElement('article'); card.className = 'card';
    const title = document.createElement('h3'); title.textContent = c.name;
    const state = document.createElement('p'); state.textContent = `${c.enabled ? '已启用配置' : '已停用'} · ${c.control ? '视频与控制' : '仅观看'}`;
    const id = document.createElement('p'); id.textContent = `RustDesk ID：${c.rustdesk_id || '未配置'}`;
    const runtime = document.createElement('p'); runtime.textContent = '正在读取运行状态…';
    card.append(title, state, id, runtime); $('channels').append(card);
    const path = `/channels/${encodeURIComponent(c.id)}`;
    const status = await api(`${path}/runtime/status`);
    runtime.textContent = status.running ? '工作进程在线' : status.runtime_configured ? '工作进程已停止或待恢复' : '管理员尚未配置运行程序';
    if (status.running) consoles.add(new window.PlatformConsole(c, api, card, notice));
    if (currentUser.admin) {
      const action = (text, callback) => { const b = document.createElement('button'); b.textContent = text;
        b.addEventListener('click', async () => { b.disabled = true; try { await callback(); await refresh(); } catch (e) { notice(e.message, true); } finally { b.disabled = false; } }); card.append(b); };
      if (!status.running) action(c.enabled ? '停用配置' : '启用配置', () => api(path, 'PUT', { ...c, enabled: !c.enabled }));
      if (status.runtime_configured && c.enabled) action(status.running ? '停止通道' : '启动通道', () => api(`${path}/runtime/${status.running ? 'stop' : 'start'}`, 'POST'));
      if (!status.running) await settings(card, c);
    }
  }
  if (!channels.length) { const p = document.createElement('p'); p.textContent = '暂无获分配的设备。'; $('channels').append(p); }
  if (currentUser.admin) {
    const [devices, resources, users] = await Promise.all([api('/discovery'), api('/resources'), api('/users')]);
    options($('register').elements.candidate_id, devices);
    options($('channel').elements.video, resources.filter(r => r.kind === 'video'));
    options($('channel').elements.control, resources.filter(r => r.kind === 'control'), '仅观看');
    options($('grant').elements.user, users.filter(u => u.active));
    options($('grant').elements.channel, channels);
  }
}
function form(id, action) {
  $(id).addEventListener('submit', async event => {
    event.preventDefault(); const button = event.submitter; if (button) button.disabled = true;
    try { await action(Object.fromEntries(new FormData($(id)))); } catch (error) { notice(error.message, true); }
    finally { if (button) button.disabled = false; }
  });
}
form('login', async values => {
  const result = await api('/auth/login', 'POST', values); token = result.token; currentUser = result.user;
  $('login').reset(); $('login').hidden = true; $('workspace').hidden = false; $('logout').hidden = false;
  $('administration').hidden = !currentUser.admin; $('identity').textContent = `${currentUser.username} · ${currentUser.admin ? '管理员' : '用户'}`;
  await refresh(); notice('已登录。请选择设备；观看与控制权限分别校验。');
});
form('register', async values => { await api('/resources/register', 'POST', values); await refresh(); notice('资源已登记。'); });
form('channel', async values => {
  await api('/channels', 'POST', { ...values, id: '', enabled: false, control: values.control || null, rustdesk_id: values.rustdesk_id || null });
  $('channel').reset(); await refresh(); notice('通道已创建，保持停用。');
});
form('user', async values => { await api('/users', 'POST', values); $('user').reset(); await refresh(); notice('用户已创建。'); });
form('grant', async values => { await api(`/grants/${encodeURIComponent(values.user)}/${encodeURIComponent(values.channel)}`, 'PUT', { role: values.role }); notice('通道权限已更新。'); });
$('revoke').addEventListener('click', async () => {
  const f = $('grant').elements; if (!f.user.value || !f.channel.value) return;
  try { await api(`/grants/${encodeURIComponent(f.user.value)}/${encodeURIComponent(f.channel.value)}`, 'DELETE'); notice('通道权限已撤销。'); }
  catch (e) { notice(e.message, true); }
});
$('refresh').addEventListener('click', () => refresh().catch(e => notice(e.message, true)));
$('logout').addEventListener('click', async () => { try { await closeConsoles(); await api('/auth/logout', 'POST'); } finally { reset(); } });
window.addEventListener('pagehide', () => { closeConsoles(); });
window.addEventListener('blur', () => { for (const c of consoles) c.release(); });
document.addEventListener('visibilitychange', () => { if (document.hidden) for (const c of consoles) c.release(); });

async function settings(card, channel) {
  const path = `/channels/${encodeURIComponent(channel.id)}/runtime/config`;
  const config = await api(path);
  const details = document.createElement('details'); const summary = document.createElement('summary'); summary.textContent = '采集与 RustDesk 设置'; details.append(summary);
  const form = document.createElement('form'); const fields = {};
  const field = (name, title, value, type = 'text') => { const label = document.createElement('label'); label.textContent = title;
    const input = document.createElement('input'); input.name = name; input.type = type; input.value = value ?? '';
    if (type === 'password') input.autocomplete = 'new-password'; label.append(input); form.append(label); fields[name] = input; };
  for (const [name, title] of [['width', '采样宽度'], ['height', '采样高度'], ['fps', '采样帧率']]) field(name, title, config.video?.[name], 'number');
  field('format', '采样格式（如 NV12、MJPEG）', config.video?.format);
  field('rendezvous_server', 'RustDesk ID 服务器', config.rustdesk.rendezvous_server);
  field('relay_server', 'RustDesk 中继服务器', config.rustdesk.relay_server);
  field('rustdesk_id', '手动输入自定义 RustDesk ID', channel.rustdesk_id);
  field('relay_key', config.rustdesk.key_set ? '中继 Key（已设置，留空保留）' : '中继 Key', '', 'password');
  field('device_password', config.rustdesk.password_set ? '自定义密码（已设置，留空保留）' : '自定义密码（至少 6 个字符）', '', 'password');
  const enabled = document.createElement('label'); const checkbox = document.createElement('input'); checkbox.type = 'checkbox'; checkbox.checked = !!config.rustdesk.enabled; enabled.append(checkbox, document.createTextNode('启用此通道 RustDesk')); form.append(enabled);
  const clear = document.createElement('label'); const clearKey = document.createElement('input'); clearKey.type = 'checkbox'; clear.append(clearKey, document.createTextNode('清空已保存的中继 Key')); form.append(clear);
  const save = document.createElement('button'); save.textContent = '保存设置'; form.append(save);
  form.addEventListener('submit', async event => { event.preventDefault(); save.disabled = true;
    try { const body = { rustdesk_enabled: checkbox.checked };
      for (const name of ['width', 'height', 'fps']) if (fields[name].value) body[name] = Number(fields[name].value);
      for (const name of ['format', 'rendezvous_server', 'relay_server']) if (fields[name].value) body[name] = fields[name].value;
      for (const name of ['relay_key', 'device_password']) if (fields[name].value) body[name] = fields[name].value;
      if (clearKey.checked) body.relay_key = '';
      if (fields.rustdesk_id.value !== (channel.rustdesk_id || '')) {
        await api(`/channels/${encodeURIComponent(channel.id)}`, 'PUT', { ...channel, rustdesk_id: fields.rustdesk_id.value });
        channel.rustdesk_id = fields.rustdesk_id.value;
      }
      await api(path, 'PATCH', body); fields.relay_key.value = ''; fields.device_password.value = ''; notice('通道设置已保存。');
    } catch (e) { notice(e.message, true); } finally { save.disabled = false; }
  });
  details.append(form); card.append(details);
}
