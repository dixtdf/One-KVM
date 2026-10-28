'use strict';
// Each console owns exactly one channel, one peer and one control lease.
window.PlatformConsole = class {
  constructor(channel, api, container, report) {
    this.id = channel.id; this.api = api; this.report = report; this.peer = null;
    this.session = null; this.control = false; this.generation = 0; this.queue = Promise.resolve();
    const element = (name, text) => { const e = document.createElement(name); if (text) e.textContent = text; return e; };
    this.video = element('video'); this.video.autoplay = true; this.video.playsInline = true; this.video.muted = true;
    this.video.tabIndex = 0; this.video.setAttribute('aria-label', `${channel.name} 远程画面，取得控制权后点击操作`);
    this.connect = element('button', '观看'); this.closeButton = element('button', '断开');
    this.controlButton = element('button', '申请控制'); this.controlButton.hidden = !channel.control;
    this.controlButton.addEventListener('pointerdown', e => e.preventDefault());
    this.state = element('p', '未连接');
    container.append(this.connect, this.closeButton, this.controlButton, this.video, this.state);
    this.connect.addEventListener('click', () => this.open().catch(e => report(e.message, true)));
    this.closeButton.addEventListener('click', () => this.close());
    this.controlButton.addEventListener('click', () => (this.control ? this.release() : this.acquire()).catch(e => report(e.message, true)));
    this.video.addEventListener('keydown', e => this.key(e, 0)); this.video.addEventListener('keyup', e => this.key(e, 1));
    this.video.addEventListener('blur', () => this.release());
    this.video.addEventListener('contextmenu', e => { if (this.control) e.preventDefault(); });
    this.video.addEventListener('pointermove', e => {
      if (!this.control || performance.now() - (this.lastMove || 0) < 40) return;
      this.lastMove = performance.now(); const rect = this.video.getBoundingClientRect();
      // Video uses contain sizing; exclude black bars when mapping coordinates.
      const ratio = Math.min(rect.width / this.video.videoWidth, rect.height / this.video.videoHeight);
      const w = this.video.videoWidth * ratio, h = this.video.videoHeight * ratio;
      if (!w || !h) return;
      const x = Math.max(0, Math.min(32767, Math.round((e.clientX - rect.left - (rect.width - w) / 2) / w * 32767)));
      const y = Math.max(0, Math.min(32767, Math.round((e.clientY - rect.top - (rect.height - h) / 2) / h * 32767)));
      this.send([2, 1, x & 255, x >> 8, y & 255, y >> 8, 0]);
    });
    for (const [event, type] of [['pointerdown', 2], ['pointerup', 3]]) this.video.addEventListener(event, e => {
      if (!this.control || e.button > 2) return; e.preventDefault(); this.video.focus();
      if (event === 'pointerdown') this.video.setPointerCapture(e.pointerId);
      this.send([2, type, 0, 0, 0, 0, e.button]);
    });
    this.video.addEventListener('pointercancel', () => this.release());
    this.video.addEventListener('wheel', e => { if (!this.control) return; e.preventDefault(); this.send([2, 4, 0, 0, 0, 0, e.deltaY > 0 ? 255 : 1]); }, { passive: false });
  }
  path(suffix) { return `/channels/${encodeURIComponent(this.id)}/${suffix}`; }
  async open() {
    await this.close(); const generation = ++this.generation; this.connect.disabled = true; this.state.textContent = '正在连接…';
    let peer;
    try {
      const servers = await this.api(this.path('video/ice-servers'));
      if (generation !== this.generation) return;
      peer = new RTCPeerConnection({ iceServers: servers.ice_servers || [] }); this.peer = peer;
      peer.ontrack = event => { if (generation === this.generation) this.video.srcObject = event.streams[0] || new MediaStream([event.track]); };
      peer.onconnectionstatechange = () => {
        if (generation !== this.generation) return;
        this.state.textContent = peer.connectionState === 'connected' ? '已连接 · 仅观看' : peer.connectionState;
        if (['failed', 'closed'].includes(peer.connectionState)) this.close();
      };
      peer.addTransceiver('video', { direction: 'recvonly' });
      // No WebRTC input data channel: keyboard/mouse must pass the gateway.
      await peer.setLocalDescription(await peer.createOffer());
      await new Promise((resolve, reject) => {
        if (peer.iceGatheringState === 'complete') return resolve();
        const timer = setTimeout(() => { peer.removeEventListener('icegatheringstatechange', check); reject(new Error('ICE 收集超时')); }, 10000);
        const check = () => { if (peer.iceGatheringState === 'complete') { clearTimeout(timer); peer.removeEventListener('icegatheringstatechange', check); resolve(); } };
        peer.addEventListener('icegatheringstatechange', check);
      });
      if (generation !== this.generation) return;
      const answer = await this.api(this.path('video/offer'), 'POST', { sdp: peer.localDescription.sdp });
      if (generation !== this.generation) { await this.api(this.path('video/close'), 'POST', { session_id: answer.session_id }); return; }
      this.session = answer.session_id; await peer.setRemoteDescription({ type: 'answer', sdp: answer.sdp });
      for (const candidate of answer.ice_candidates || []) await peer.addIceCandidate(candidate);
    } catch (e) { if (generation === this.generation) { await this.close(); this.state.textContent = '连接失败'; } throw e; }
    finally { this.connect.disabled = false; if (generation !== this.generation && peer) peer.close(); }
  }
  async acquire() {
    if (this.control || this.acquiring) return;
    if (!this.peer || this.peer.connectionState !== 'connected') throw new Error('请先连接视频');
    this.acquiring = true;
    try {
    const generation = this.generation;
    await this.api(this.path('control/acquire'), 'POST');
    if (generation !== this.generation || !this.peer) { await this.api(this.path('control/release'), 'POST'); return; }
    this.control = true; this.controlButton.textContent = '释放控制';
    this.state.textContent = '已取得控制权'; this.video.focus();
    this.renew = setInterval(() => this.api(this.path('control/acquire'), 'POST').catch(() => this.release()), 5000);
    } finally { this.acquiring = false; }
  }
  async release() {
    clearInterval(this.renew); if (!this.control) return;
    this.control = false; this.controlButton.textContent = '申请控制';
    // Pending queued packets recheck control before delivery. Wait for any
    // in-flight packet before neutral release so input cannot follow release.
    await this.queue.catch(() => {});
    try { await this.api(this.path('control/release'), 'POST'); } catch (_) { /* server expiry/revocation also resets */ }
  }
  send(packet) {
    if ((this.pending || 0) >= 32) { this.report('输入通道拥塞，已释放控制权', true); this.release(); return; }
    this.pending = (this.pending || 0) + 1;
    const generation = this.generation;
    this.queue = this.queue.then(async () => {
      if (!this.control || generation !== this.generation) return;
      await this.api(this.path('control/input'), 'POST', packet);
    }).catch(e => { this.report(e.message, true); this.release(); }).finally(() => { this.pending--; });
  }
  key(event, up) {
    if (!this.control) return;
    let key = window.PlatformConsole.keyCode(event.code); if (key === null) return;
    event.preventDefault(); if (event.repeat) return;
    const modifiers = (event.ctrlKey ? 1 : 0) | (event.shiftKey ? 2 : 0) | (event.altKey ? 4 : 0) | (event.metaKey ? 8 : 0);
    this.send([1, up, key, modifiers]);
  }
  static keyCode(code) {
    if (/^Key[A-Z]$/.test(code)) return code.charCodeAt(3) - 65 + 4;
    if (/^Digit[1-9]$/.test(code)) return Number(code[5]) + 29;
    if (/^F([1-9]|1[0-2])$/.test(code)) return Number(code.slice(1)) + 57;
    return ({ Digit0: 39, Enter: 40, Escape: 41, Backspace: 42, Tab: 43, Space: 44,
      Minus: 45, Equal: 46, BracketLeft: 47, BracketRight: 48, Backslash: 49, Semicolon: 51,
      Quote: 52, Backquote: 53, Comma: 54, Period: 55, Slash: 56, CapsLock: 57,
      Insert: 73, Home: 74, PageUp: 75, Delete: 76, End: 77, PageDown: 78,
      ArrowRight: 79, ArrowLeft: 80, ArrowDown: 81, ArrowUp: 82,
      ControlLeft: 224, ShiftLeft: 225, AltLeft: 226, MetaLeft: 227,
      ControlRight: 228, ShiftRight: 229, AltRight: 230, MetaRight: 231 })[code] ?? null;
  }
  async close() {
    ++this.generation; await this.release();
    const session = this.session; this.session = null;
    const peer = this.peer; this.peer = null; if (peer) peer.close(); this.video.srcObject = null;
    if (session) { try { await this.api(this.path('video/close'), 'POST', { session_id: session }); } catch (_) {} }
    this.state.textContent = '未连接'; this.connect.disabled = false;
  }
};
