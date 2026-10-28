'use strict';
let token = '';
let currentUser = null;
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
  const channels = await api('/channels');
  $('channels').replaceChildren();
  for (const c of channels) {
    const card = document.createElement('article'); card.className = 'card';
    const title = document.createElement('h3'); title.textContent = c.name;
    const state = document.createElement('p'); state.textContent = `${c.enabled ? '已启用配置' : '已停用'} · ${c.control ? '视频与控制' : '仅观看'}`;
    const id = document.createElement('p'); id.textContent = `RustDesk ID：${c.rustdesk_id || '未配置'}`;
    const runtime = document.createElement('p'); runtime.textContent = '工作进程接入开发中，暂未开放连接。';
    card.append(title, state, id, runtime); $('channels').append(card);
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
  await refresh(); notice('已登录。实验阶段仅提供通道与授权管理。');
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
$('logout').addEventListener('click', async () => { try { await api('/auth/logout', 'POST'); } finally { reset(); } });
