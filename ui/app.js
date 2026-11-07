import { api, initApi, listen } from './api.js';
import { icon, hydrateIcons, setIcon } from './icons.js';
import { player } from './player.js';
import { t, translateDom, LANGS, lang, setLang, locale } from './i18n.js';

// ---------------------------------------------------------------- helpers
const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => [...r.querySelectorAll(s)];
const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
const fmt = (sec) => {
  if (!isFinite(sec) || sec < 0) return '0:00';
  sec = Math.floor(sec);
  const h = Math.floor(sec / 3600), m = Math.floor((sec % 3600) / 60), s = sec % 60;
  return h ? `${h}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}` : `${m}:${String(s).padStart(2, '0')}`;
};
const totalTime = (tracks) => {
  const s = tracks.reduce((a, x) => a + (x.duration || 0), 0);
  const h = Math.floor(s / 3600), m = Math.round((s % 3600) / 60);
  return h ? t('time.hm', { h, m }) : t('time.m', { m });
};
const tracksLabel = (n) => t('tracks', { n });
const kindLabel = (k) => (k ? t(`kind.${k}`) : '');
const compact = (n) => new Intl.NumberFormat(locale(), { notation: 'compact', maximumFractionDigits: 1 }).format(n);
const subsLabel = (n) => (n == null ? '' : t('subscribers', { n: compact(n) }));
const artistLinks = (artists = []) => artists.map((a) =>
  a.id ? `<a class="link" data-go="artist" data-id="${esc(a.id)}">${esc(a.name)}</a>` : esc(a.name)).join(', ');
const artistNames = (artists = []) => artists.map((a) => a.name).join(', ');
const errText = (e) => (typeof e === 'string' ? e : e?.message || JSON.stringify(e));
const debounce = (fn, ms) => { let h; return (...a) => { clearTimeout(h); h = setTimeout(() => fn(...a), ms); }; };

function toast(msg, kind = '') {
  const el = document.createElement('div');
  el.className = `toast ${kind}`;
  el.textContent = msg;
  $('#toasts').appendChild(el);
  setTimeout(() => el.classList.add('out'), 2800);
  setTimeout(() => el.remove(), 3200);
}

// ---------------------------------------------------------------- state
const state = {
  liked: new Set(),
  downloaded: new Set(),
  dlProgress: new Map(), // id -> {state, progress}
  playlists: [],
  remotePlaylists: [],
  settings: {},
  lists: new Map(), // list id -> tracks (for event delegation)
  listSeq: 0,
};

function registerList(tracks) {
  const id = `l${++state.listSeq}`;
  state.lists.set(id, tracks);
  if (state.lists.size > 60) state.lists.delete(state.lists.keys().next().value);
  return id;
}

async function refreshLibrary() {
  const [liked, dl, pls] = await Promise.all([api.likedIds(), api.downloadedIds(), api.playlists()]);
  state.liked = new Set(liked);
  state.downloaded = new Set(dl);
  state.playlists = pls;
  renderSidebar();
}

async function refreshRemote() {
  try { state.remotePlaylists = await api.remotePlaylists(); } catch { state.remotePlaylists = []; }
  renderSidebar();
}

// ---------------------------------------------------------------- router
const nav = { stack: [], pos: -1 };
let renderToken = 0;

function go(view, params = {}, push = true) {
  if (push) {
    nav.stack = nav.stack.slice(0, nav.pos + 1);
    nav.stack.push({ view, params, scroll: 0 });
    nav.pos = nav.stack.length - 1;
  }
  render();
}

async function render() {
  const entry = nav.stack[nav.pos];
  const token = ++renderToken;
  const el = $('#view');
  $$('.nav-item').forEach((a) => a.classList.toggle('active', a.dataset.nav === entry.view
    || (entry.view === 'local-playlist' && a.dataset.pl == entry.params.id)
    || (entry.view === 'playlist' && a.dataset.rpl === entry.params.id)));
  $('#nav-back').disabled = nav.pos <= 0;
  $('#nav-fwd').disabled = nav.pos >= nav.stack.length - 1;
  el.innerHTML = '<div class="loading">' + icon('spinner', 'spin') + '</div>';
  try {
    const html = await (views[entry.view] || views.home)(entry.params);
    if (token !== renderToken) return;
    el.innerHTML = html;
    $('#main').scrollTop = entry.scroll || 0;
    afterRender[entry.view]?.(entry.params);
  } catch (e) {
    if (token !== renderToken) return;
    console.error(e);
    el.innerHTML = `<div class="empty"><h2>${esc(t('err.title'))}</h2><p>${esc(errText(e))}</p>
      <button class="pill-btn" onclick="location.reload()">${esc(t('err.reload'))}</button></div>`;
  }
}

$('#main').addEventListener('scroll', () => {
  const e = nav.stack[nav.pos];
  if (e) e.scroll = $('#main').scrollTop;
  $('#topbar').classList.toggle('scrolled', $('#main').scrollTop > 10);
});

// ---------------------------------------------------------------- components
const dlMarkDone = () => `<span class="dl-mark" title="${esc(t('tr.downloaded'))}">${icon('downloaded')}</span>`;
const dlMarkBusy = (p) => `<span class="dl-mark busy" title="${esc(t('tr.downloading'))}">${Math.round((p || 0) * 100)}%</span>`;

function trackRows(tracks, opts = {}) {
  const lid = registerList(tracks);
  const cur = player.current?.id;
  const rows = tracks.map((x, i) => {
    const liked = state.liked.has(x.id);
    const prog = state.dlProgress.get(x.id);
    const dlIcon = state.downloaded.has(x.id) ? dlMarkDone() : prog && prog.state !== 'error' ? dlMarkBusy(prog.progress) : '';
    return `<div class="trow ${x.id === cur ? 'playing' : ''}" data-list="${lid}" data-idx="${i}" data-tid="${esc(x.id)}">
      <div class="tnum"><span class="n">${opts.numbers === false ? '' : i + 1}</span><span class="eq"><b></b><b></b><b></b></span>${icon('play', 'pl')}</div>
      <div class="tmain">
        ${opts.covers === false ? '' : `<img class="tcover" loading="lazy" src="${esc(x.cover || '')}" alt="" onerror="this.style.visibility='hidden'">`}
        <div class="ttext"><div class="ttitle">${esc(x.title)}</div><div class="tartist">${dlIcon}${artistLinks(x.artists)}</div></div>
      </div>
      ${opts.album === false ? '' : `<div class="talbum">${x.album ? `<a class="link" data-go="album" data-id="${esc(x.album.id)}">${esc(x.album.name)}</a>` : ''}</div>`}
      <div class="tright">
        <button class="icon-btn like ${liked ? 'on' : ''}" data-act="like" title="${esc(t('tr.like'))}">${icon(liked ? 'heart-fill' : 'heart')}</button>
        <span class="tdur">${x.duration ? fmt(x.duration) : ''}</span>
        <button class="icon-btn more" data-act="menu" title="${esc(t('tr.more'))}">${icon('more')}</button>
      </div>
    </div>`;
  }).join('');
  const noAlbum = opts.album === false;
  const head = `<div class="thead ${noAlbum ? 'noalbum' : ''}"><div>#</div><div>${esc(t('col.title'))}</div>${noAlbum ? '' : `<div>${esc(t('col.album'))}</div>`}<div class="tright">${icon('clock')}</div></div>`;
  return `<div class="tracklist ${noAlbum ? 'noalbum' : ''}" data-ctx='${esc(JSON.stringify(opts.ctx || null))}'>${opts.header === false ? '' : head}${rows}</div>`;
}

function card(kind, item) {
  const round = kind === 'artist';
  const title = item.title || item.name;
  let sub = '';
  if (kind === 'album') sub = [item.year, kindLabel(item.kind), artistNames(item.artists)].filter(Boolean).join(' · ');
  if (kind === 'playlist') sub = item.author || (item.count ? tracksLabel(item.count) : t('kind.playlist'));
  if (kind === 'artist') sub = item.subscribers != null ? subsLabel(item.subscribers) : t('kind.artist');
  if (kind === 'local') sub = tracksLabel(item.count || 0);
  const cover = item.cover
    ? `<img loading="lazy" src="${esc(item.cover)}" alt="" onerror="this.remove()">`
    : `<div class="ph">${icon(kind === 'artist' ? 'user' : 'music')}</div>`;
  return `<div class="card" data-go="${kind === 'local' ? 'local-playlist' : kind}" data-id="${esc(item.id)}">
    <div class="card-art ${round ? 'round' : ''}">${cover}
      ${kind !== 'artist' ? `<button class="card-play" data-act="play-collection" data-kind="${kind}" data-id="${esc(item.id)}" title="${esc(t('act.play'))}">${icon('play')}</button>` : ''}
    </div>
    <div class="card-title">${esc(title)}</div>
    <div class="card-sub">${esc(sub)}</div>
  </div>`;
}

const shelf = (title, items, kind) => !items?.length ? '' :
  `<section class="shelf"><h2>${esc(title)}</h2><div class="grid">${items.map((i) => card(kind, i)).join('')}</div></section>`;

function collectionHeader({ cover, kind, title, meta, round = false, actions = '', phClass = '' }) {
  return `<div class="hero">
    <div class="hero-art ${round ? 'round' : ''}">${cover ? `<img src="${esc(cover)}" alt="">` : `<div class="ph ${phClass}">${icon(phClass ? (phClass === 'liked-cover' ? 'heart-fill' : 'download') : 'music')}</div>`}</div>
    <div class="hero-text">
      <div class="hero-kind">${esc(kind)}</div>
      <h1 class="${title.length > 30 ? 'long' : ''}">${esc(title)}</h1>
      <div class="hero-meta">${meta}</div>
    </div>
  </div>
  <div class="actions">${actions}</div>`;
}

const playBtn = (act, extra = '') => `<button class="play-big" data-act="${act}" ${extra} title="${esc(t('act.play'))}">${icon('play')}</button>`;
const actBtn = (act, ic, title, extra = '') => `<button class="icon-btn big" data-act="${act}" ${extra} title="${esc(title)}">${icon(ic)}</button>`;
const collectionActions = () => playBtn('play-page') + actBtn('shuffle-page', 'shuffle', t('act.shuffle'))
  + actBtn('download-page', 'download', t('act.downloadAll'));

// cached data of the currently rendered collection page, used by action buttons
let pageTracks = [];
let pageCtx = null;

// ---------------------------------------------------------------- views
const views = {
  async home() {
    const [hist, home] = await Promise.all([
      api.history().catch(() => []),
      api.home().catch((e) => ({ sections: [], error: errText(e) })),
    ]);
    const hour = new Date().getHours();
    const greet = t(hour < 5 ? 'greet.night' : hour < 12 ? 'greet.morning' : hour < 18 ? 'greet.day' : 'greet.evening');
    const recent = [];
    const seen = new Set();
    for (const x of hist) { if (!seen.has(x.id)) { seen.add(x.id); recent.push(x); } if (recent.length >= 8) break; }
    const quick = [
      { go: 'liked', title: t('lib.liked'), cls: 'liked-cover', ic: 'heart-fill' },
      ...state.playlists.slice(0, 5).map((p) => ({ go: 'local-playlist', id: p.id, title: p.name, cover: p.cover })),
    ];
    let html = `<div class="page home"><h1 class="greet">${esc(greet)}</h1>
      <div class="quick">${quick.map((q) => `<div class="quick-item" data-go="${q.go}" data-id="${esc(q.id ?? '')}">
        ${q.cover ? `<img src="${esc(q.cover)}" alt="">` : `<span class="qc ${q.cls || ''}">${icon(q.ic || 'music')}</span>`}
        <span>${esc(q.title)}</span></div>`).join('')}</div>`;
    if (recent.length) html += `<section class="shelf"><h2>${esc(t('home.recent'))}</h2>${trackRows(recent, { header: false, album: false })}</section>`;
    for (const s of home.sections || []) {
      const title = t(`sec.${s.title}`);
      if (s.tracks?.length) html += `<section class="shelf"><h2>${esc(title)}</h2>${trackRows(s.tracks.slice(0, 10), { header: false })}</section>`;
      html += shelf(title, s.albums, 'album');
      html += shelf(title, s.playlists, 'playlist');
      html += shelf(title, s.artists, 'artist');
    }
    if (home.error) html += `<div class="note">${esc(t('home.recoFail', { e: home.error }))}</div>`;
    return html + '</div>';
  },

  async search({ q }) {
    if (!q) {
      const hist = (() => { try { return JSON.parse(localStorage.getItem('searches') || '[]'); } catch { return []; } })();
      return `<div class="page"><h1>${esc(t('search.title'))}</h1>
        ${hist.length ? `<h2>${esc(t('search.recent'))}</h2><div class="chips">${hist.map((s) => `<button class="chip" data-search="${esc(s)}">${esc(s)}</button>`).join('')}</div>`
        : `<p class="muted">${esc(t('search.hint'))}</p>`}</div>`;
    }
    const r = await api.search(q);
    const top = r.tracks.slice(0, 20);
    let html = `<div class="page"><h1 class="search-h">${esc(t('search.results', { q }))}</h1>`;
    if (!top.length && !r.albums.length && !r.artists.length && !r.playlists.length) {
      return html + `<p class="muted">${esc(t('search.none'))}</p></div>`;
    }
    if (r.artists[0]) {
      const a = r.artists[0];
      html += `<div class="top-result-row"><div class="top-result" data-go="artist" data-id="${esc(a.id)}">
        ${a.cover ? `<img class="round" src="${esc(a.cover)}" alt="">` : ''}
        <div class="tr-name">${esc(a.name)}</div><div class="tr-kind">${esc(t('kind.artist'))}</div></div>
        <div class="top-tracks"><h2>${esc(t('search.songs'))}</h2>${trackRows(top.slice(0, 5), { header: false, album: false })}</div></div>`;
      if (top.length > 5) html += `<section class="shelf">${trackRows(top.slice(5), { header: false })}</section>`;
    } else if (top.length) {
      html += `<section class="shelf"><h2>${esc(t('search.songs'))}</h2>${trackRows(top)}</section>`;
    }
    html += shelf(t('search.artists'), r.artists, 'artist');
    html += shelf(t('search.albums'), r.albums, 'album');
    html += shelf(t('search.playlists'), r.playlists, 'playlist');
    return html + '</div>';
  },

  async album({ id }) {
    const a = await api.album(id);
    pageTracks = a.tracks; pageCtx = { kind: 'album', id, title: a.title };
    return `<div class="page">${collectionHeader({
      cover: a.cover, kind: kindLabel(a.kind) || t('kind.album'), title: a.title,
      meta: `${artistLinks(a.artists)}${a.year ? ` · ${a.year}` : ''} · ${tracksLabel(a.tracks.length)}, ${totalTime(a.tracks)}`,
      actions: collectionActions() + actBtn('save-page', 'plus', t('act.saveAsPlaylist')),
    })}${trackRows(a.tracks, { covers: false, album: false, ctx: pageCtx })}
    ${a.description ? `<p class="desc">${esc(a.description)}</p>` : ''}</div>`;
  },

  async artist({ id }) {
    const a = await api.artist(id);
    pageTracks = a.tracks; pageCtx = { kind: 'artist', id, title: a.name };
    return `<div class="page artist">
      <div class="artist-hero" style="${a.cover ? `--bg-img:url('${esc(a.cover)}')` : ''}">
        <div class="hero-kind">${esc(t('kind.artist'))}</div><h1>${esc(a.name)}</h1>
        ${a.subscribers != null ? `<div class="hero-meta">${esc(subsLabel(a.subscribers))}</div>` : ''}
      </div>
      <div class="actions">${playBtn('play-page')}${actBtn('radio-artist', 'radio', t('act.artistRadio'))}</div>
      ${a.tracks.length ? `<section class="shelf"><h2>${esc(t('artist.popular'))}</h2>${trackRows(a.tracks.slice(0, 10), { header: false, ctx: pageCtx })}</section>` : ''}
      ${shelf(t('artist.albums'), a.albums, 'album')}
      ${shelf(t('artist.singles'), a.singles, 'album')}
      ${shelf(t('artist.playlists'), a.playlists, 'playlist')}
      ${shelf(t('artist.similar'), a.similar, 'artist')}
      ${a.description ? `<section class="shelf"><h2>${esc(t('artist.about'))}</h2><p class="desc">${esc(a.description)}</p></section>` : ''}
    </div>`;
  },

  async playlist({ id }) {
    const p = await api.playlist(id);
    pageTracks = p.tracks; pageCtx = { kind: 'playlist', id, title: p.title };
    return `<div class="page">${collectionHeader({
      cover: p.cover, kind: t('kind.playlist'), title: p.title,
      meta: `${p.author ? `<b>${esc(p.author)}</b> · ` : ''}${tracksLabel(p.tracks.length)}, ${totalTime(p.tracks)}`,
      actions: collectionActions() + actBtn('save-page', 'plus', t('act.saveToLibrary')),
    })}${p.description ? `<p class="desc top">${esc(p.description)}</p>` : ''}${trackRows(p.tracks, { ctx: pageCtx })}</div>`;
  },

  async 'local-playlist'({ id }) {
    const p = await api.localPlaylist(Number(id));
    pageTracks = p.tracks; pageCtx = { kind: 'local', id: Number(id), title: p.name };
    return `<div class="page">${collectionHeader({
      cover: p.tracks[0]?.cover, kind: t('kind.playlist'), title: p.name,
      meta: `${tracksLabel(p.tracks.length)}${p.tracks.length ? `, ${totalTime(p.tracks)}` : ''}`,
      actions: collectionActions() + actBtn('rename-local', 'edit', t('act.rename')) + actBtn('delete-local', 'trash', t('act.deletePlaylist')),
    })}${p.tracks.length ? trackRows(p.tracks, { ctx: pageCtx }) : `<p class="muted pad">${esc(t('local.empty'))}</p>`}</div>`;
  },

  async liked() {
    const tracks = await api.likedTracks();
    pageTracks = tracks; pageCtx = { kind: 'liked', title: t('lib.liked') };
    return `<div class="page">${collectionHeader({
      cover: null, kind: t('kind.playlist'), title: t('lib.liked'), meta: tracksLabel(tracks.length),
      actions: collectionActions(), phClass: 'liked-cover',
    })}
    ${tracks.length ? trackRows(tracks, { ctx: pageCtx }) : `<p class="muted pad">${esc(t('liked.empty'))}</p>`}</div>`;
  },

  async downloads() {
    const items = await api.downloads();
    const tracks = items.map((d) => d.track);
    pageTracks = tracks; pageCtx = { kind: 'downloads', title: t('lib.downloads') };
    const size = items.reduce((a, d) => a + (d.size || 0), 0);
    const active = [...state.dlProgress.values()].filter((p) => p.state === 'queued' || p.state === 'downloading');
    return `<div class="page">${collectionHeader({
      cover: null, kind: t('kind.offline'), title: t('lib.downloads'),
      meta: `${tracksLabel(tracks.length)} · ${(size / 1048576).toFixed(0)} MB · <a class="link" data-act="open-dl-dir">${esc(state.settings.download_dir || '')}</a>`,
      actions: playBtn('play-page') + actBtn('shuffle-page', 'shuffle', t('act.shuffle')), phClass: 'dl-cover',
    })}
    ${active.length ? `<div class="note">${esc(t('dl.active', { n: active.length }))}</div>` : ''}
    ${tracks.length ? trackRows(tracks, { ctx: pageCtx }) : `<p class="muted pad">${esc(t('dl.empty'))}</p>`}</div>`;
  },

  async history() {
    const tracks = await api.history();
    pageTracks = tracks; pageCtx = null;
    return `<div class="page"><h1>${esc(t('lib.history'))}</h1>${tracks.length ? trackRows(tracks, { numbers: false }) : `<p class="muted">${esc(t('history.empty'))}</p>`}</div>`;
  },

  async settings() {
    const s = await api.settings();
    state.settings = s;
    return `<div class="page settings"><h1>${esc(t('set.title'))}</h1>
      <section class="set-block">
        <h2>${esc(t('set.language'))}</h2>
        <div class="row"><select id="lang-select" class="text-in select">
          ${Object.entries(LANGS).map(([k, v]) => `<option value="${k}" ${k === lang ? 'selected' : ''}>${esc(v)}</option>`).join('')}
        </select></div>
      </section>
      <section class="set-block">
        <h2>${esc(t('set.account'))}</h2>
        ${s.logged_in ? `<p>${esc(t('set.loggedIn'))}${s.account_name ? `: <b>${esc(s.account_name)}</b>` : ''}.</p>
          <div class="row"><button class="pill-btn primary" data-act="sync">${icon('sync')} ${esc(t('set.sync'))}</button>
          <button class="pill-btn" data-act="logout">${esc(t('set.logout'))}</button></div>`
        : `<p class="muted">${esc(t('set.loginHint'))}</p>
          <ol class="muted small"><li>${esc(t('set.step1'))}</li><li>${esc(t('set.step2'))}</li><li>${esc(t('set.step3'))}</li></ol>
          <div class="row"><label class="pill-btn">${esc(t('set.pickFile'))}<input type="file" id="cookie-file" accept=".txt" hidden></label></div>
          <textarea id="cookie-text" placeholder="${esc(t('set.cookiePh'))}"></textarea>
          <div class="row"><button class="pill-btn primary" data-act="login">${esc(t('set.login'))}</button></div>`}
      </section>
      <section class="set-block">
        <h2>${esc(t('set.downloads'))}</h2>
        <p class="muted small">${esc(t('set.dlHint'))}</p>
        <div class="row"><input id="dl-dir" class="text-in" value="${esc(s.download_dir)}" spellcheck="false">
          <button class="pill-btn" data-act="save-dl-dir">${esc(t('set.save'))}</button>
          <button class="pill-btn" data-act="open-dl-dir">${esc(t('set.openFolder'))}</button></div>
      </section>
      <section class="set-block">
        <h2>${esc(t('set.playback'))}</h2>
        <label class="check"><input type="checkbox" id="autoplay" ${player.autoplay ? 'checked' : ''}> ${esc(t('set.autoplay'))}</label>
      </section>
      ${s.ytdlp_supported === false ? '' : `<section class="set-block">
        <h2>${esc(t('set.ytdlp'))}</h2>
        <p class="muted small">${esc(t('set.ytdlpHint'))}</p>
        <p>${s.ytdlp ? `${esc(t('set.ytdlpFound'))} <code>${esc(s.ytdlp)}</code>` : esc(t('set.ytdlpMissing'))}</p>
        <div class="row"><button class="pill-btn" data-act="install-ytdlp">${esc(t(s.ytdlp ? 'set.ytdlpUpdate' : 'set.ytdlpInstall'))}</button></div>
      </section>`}
    </div>`;
  },
};

const afterRender = {
  search({ q }) { if (q) $('#search-input').value = q; },
  settings() {
    $('#lang-select')?.addEventListener('change', (e) => setLang(e.target.value));
    $('#cookie-file')?.addEventListener('change', async (e) => {
      const f = e.target.files[0];
      if (f) $('#cookie-text').value = await f.text();
    });
    $('#autoplay')?.addEventListener('change', (e) => {
      player.autoplay = e.target.checked;
      localStorage.setItem('autoplay', JSON.stringify(player.autoplay));
    });
  },
};

// ---------------------------------------------------------------- sidebar
function renderSidebar() {
  $('#liked-count').textContent = `${t('lib.playlist')} · ${tracksLabel(state.liked.size)}`;
  $('#dl-count').textContent = `${t('lib.offline')} · ${tracksLabel(state.downloaded.size)}`;
  $('#playlist-list').innerHTML = state.playlists.map((p) => `
    <a class="nav-item lib-item" data-nav="local-playlist" data-pl="${p.id}">
      <span class="lib-cover">${p.cover ? `<img src="${esc(p.cover)}" alt="">` : icon('music')}</span>
      <span class="lib-text"><b>${esc(p.name)}</b><small>${esc(t('lib.playlist'))} · ${tracksLabel(p.count)}</small></span></a>`).join('');
  $('#remote-playlists').innerHTML = state.remotePlaylists.length
    ? '<div class="lib-sep">YouTube Music</div>' + state.remotePlaylists.map((p) => `
    <a class="nav-item lib-item" data-nav="playlist" data-rpl="${esc(p.id)}">
      <span class="lib-cover">${p.cover ? `<img src="${esc(p.cover)}" alt="">` : icon('music')}</span>
      <span class="lib-text"><b>${esc(p.title)}</b><small>${esc(p.author || t('lib.playlist'))}</small></span></a>`).join('') : '';
  $('#account-btn').textContent = state.settings.logged_in ? (state.settings.account_name || t('top.account')) : t('top.login');
}

// ---------------------------------------------------------------- right panel
let rpTab = 'queue';
function openPanel(tab) {
  const rp = $('#rightpanel');
  if (!rp.classList.contains('hidden') && rpTab === tab) { rp.classList.add('hidden'); syncPanelBtns(); return; }
  rpTab = tab;
  rp.classList.remove('hidden');
  $$('.rp-tab').forEach((b) => b.classList.toggle('active', b.dataset.tab === tab));
  renderPanel();
  syncPanelBtns();
}
function syncPanelBtns() {
  const open = !$('#rightpanel').classList.contains('hidden');
  $('#btn-queue').classList.toggle('on', open && rpTab === 'queue');
  $('#btn-lyrics').classList.toggle('on', open && rpTab === 'lyrics');
}

let lyricsFor = null;
async function renderPanel() {
  if ($('#rightpanel').classList.contains('hidden')) return;
  const body = $('#rp-body');
  if (rpTab === 'queue') {
    lyricsFor = null;
    const q = player.queue, i = player.index;
    const lid = registerList(q);
    const row = (x, idx) => `<div class="qrow ${idx === i ? 'playing' : ''}" data-qidx="${idx}" data-list="${lid}" data-idx="${idx}">
      <img src="${esc(x.cover || '')}" alt="" loading="lazy"><div class="ttext"><div class="ttitle">${esc(x.title)}</div>
      <div class="tartist">${esc(artistNames(x.artists))}</div></div>
      ${idx !== i ? `<button class="icon-btn" data-act="q-remove">${icon('close')}</button>` : ''}</div>`;
    const nextTitle = player.context?.title ? t('rp.nextFrom', { title: player.context.title }) : t('rp.next');
    body.innerHTML = !q.length ? `<p class="muted pad">${esc(t('rp.emptyQueue'))}</p>`
      : `<h3>${esc(t('rp.nowPlaying'))}</h3>${q[i] ? row(q[i], i) : ''}
        ${q.length > i + 1 ? `<h3>${esc(nextTitle)}</h3>${q.slice(i + 1, i + 101).map((x, k) => row(x, i + 1 + k)).join('')}` : ''}`;
  } else {
    const cur = player.current;
    if (!cur) { body.innerHTML = `<p class="muted pad">${esc(t('rp.nothing'))}</p>`; return; }
    if (lyricsFor === cur.id) return;
    lyricsFor = cur.id;
    body.innerHTML = '<div class="loading">' + icon('spinner', 'spin') + '</div>';
    try {
      const l = await api.lyrics(cur.id);
      if (player.current?.id !== cur.id) return;
      body.innerHTML = l ? `<div class="lyrics">${esc(l.body).replace(/\n/g, '<br>')}</div><p class="muted small pad">${esc(l.footer || '')}</p>`
        : `<p class="muted pad">${esc(t('rp.noLyrics'))}</p>`;
    } catch (e) {
      lyricsFor = null;
      body.innerHTML = `<p class="muted pad">${esc(t('rp.lyricsFail', { e: errText(e) }))}</p>`;
    }
  }
}

// ---------------------------------------------------------------- context menu
const menu = $('#ctxmenu');
function showMenu(x, y, items) {
  menu.innerHTML = items.map((it, idx) => it === '-' ? '<div class="sep"></div>'
    : it.sub ? `<div class="mi has-sub">${esc(it.label)}<span>${icon('chev-right')}</span><div class="submenu">${it.sub.map((s, k) =>
      `<div class="mi" data-mi="${k}" data-parent="${idx}">${esc(s.label)}</div>`).join('')}</div></div>`
    : `<div class="mi" data-mi="${idx}">${esc(it.label)}</div>`).join('');
  menu._items = items;
  menu.classList.remove('hidden');
  const r = menu.getBoundingClientRect();
  menu.style.left = `${Math.max(8, Math.min(x, innerWidth - r.width - 8))}px`;
  menu.style.top = `${Math.max(8, Math.min(y, innerHeight - r.height - 8))}px`;
  menu.classList.toggle('flip', x + r.width * 2 > innerWidth);
}
const hideMenu = () => menu.classList.add('hidden');
menu.addEventListener('click', (e) => {
  const mi = e.target.closest('.mi[data-mi]');
  if (!mi) return;
  const items = menu._items;
  const it = mi.dataset.parent != null ? items[mi.dataset.parent].sub[mi.dataset.mi] : items[mi.dataset.mi];
  hideMenu();
  it?.run();
});
document.addEventListener('mousedown', (e) => { if (!menu.contains(e.target)) hideMenu(); });
window.addEventListener('blur', hideMenu);

function trackMenu(x, ctx, idx, px, py) {
  const items = [
    { label: t('menu.playNext'), run: () => { player.playNext([x]); toast(t('toast.playNext')); } },
    { label: t('menu.addQueue'), run: () => { player.addToQueue([x]); toast(t('toast.queued')); } },
    { label: t('menu.radio'), run: () => startRadio(x) },
    '-',
    { label: t('menu.addPlaylist'), sub: [
      { label: t('menu.newPlaylist'), run: () => newPlaylist([x]) },
      ...state.playlists.map((p) => ({ label: p.name, run: () => addTo(p, [x]) })),
    ] },
    { label: t(state.liked.has(x.id) ? 'menu.unlike' : 'menu.like'), run: () => toggleLike(x) },
    state.downloaded.has(x.id)
      ? { label: t('menu.removeDownload'), run: () => deleteDownload(x) }
      : { label: t('menu.download'), run: () => downloadTracks([x]) },
    '-',
  ];
  if (x.artists?.[0]?.id) items.push({ label: t('menu.artist'), run: () => go('artist', { id: x.artists[0].id }) });
  if (x.album?.id) items.push({ label: t('menu.album'), run: () => go('album', { id: x.album.id }) });
  items.push({ label: t('menu.copy'), run: () => { navigator.clipboard.writeText(`https://music.youtube.com/watch?v=${x.id}`); toast(t('toast.copied')); } });
  if (ctx?.kind === 'local') {
    items.push('-', { label: t('menu.removeFromPlaylist'), run: async () => {
      await api.removeFromPlaylist(ctx.id, idx); await refreshLibrary(); render();
    } });
  }
  showMenu(px, py, items);
}

// ---------------------------------------------------------------- actions
async function toggleLike(x) {
  const liked = !state.liked.has(x.id);
  try {
    await api.setLiked(x, liked);
    liked ? state.liked.add(x.id) : state.liked.delete(x.id);
    $$(`.trow[data-tid="${CSS.escape(x.id)}"] .like`).forEach((b) => { b.classList.toggle('on', liked); b.innerHTML = icon(liked ? 'heart-fill' : 'heart'); });
    renderSidebar();
    updateNowPlaying();
    toast(t(liked ? 'toast.liked' : 'toast.unliked'));
  } catch (e) { toast(errText(e), 'err'); }
}

async function downloadTracks(tracks) {
  const todo = tracks.filter((x) => !state.downloaded.has(x.id));
  if (!todo.length) return toast(t('toast.alreadyDl'));
  for (const x of todo) state.dlProgress.set(x.id, { state: 'queued', progress: 0 });
  try { await api.download(todo); toast(t('toast.dlStarted', { tracks: tracksLabel(todo.length) })); } catch (e) { toast(errText(e), 'err'); }
}

async function deleteDownload(x) {
  await api.deleteDownload(x.id);
  state.downloaded.delete(x.id);
  renderSidebar();
  if (nav.stack[nav.pos].view === 'downloads') render();
  toast(t('toast.dlRemoved'));
}

async function startRadio(x) {
  toast(t('toast.radio'));
  try {
    const tracks = await api.radio(x.id);
    const list = [x, ...tracks.filter((y) => y.id !== x.id)];
    player.playList(list, 0, { kind: 'radio', title: t('radio.title', { title: x.title }) });
  } catch (e) { toast(errText(e), 'err'); }
}

function promptDialog(title, value = '') {
  return new Promise((resolve) => {
    const root = $('#modal-root');
    root.innerHTML = `<div class="modal-bg"><div class="modal"><h2>${esc(title)}</h2>
      <input class="text-in" id="modal-in" value="${esc(value)}" spellcheck="false">
      <div class="row end"><button class="pill-btn" id="m-cancel">${esc(t('dlg.cancel'))}</button><button class="pill-btn primary" id="m-ok">${esc(t('dlg.save'))}</button></div></div></div>`;
    const inp = $('#modal-in');
    inp.focus(); inp.select();
    const done = (v) => { root.innerHTML = ''; resolve(v); };
    $('#m-ok').onclick = () => done(inp.value.trim() || null);
    $('#m-cancel').onclick = () => done(null);
    $('.modal-bg').onclick = (e) => { if (e.target.classList.contains('modal-bg')) done(null); };
    inp.onkeydown = (e) => { if (e.key === 'Enter') done(inp.value.trim() || null); if (e.key === 'Escape') done(null); };
  });
}

async function newPlaylist(tracks = []) {
  const name = await promptDialog(t('dlg.newPlaylist'), t('dlg.defaultName', { n: state.playlists.length + 1 }));
  if (!name) return;
  const id = await api.createPlaylist(name, tracks);
  await refreshLibrary();
  toast(t(tracks.length ? 'toast.createdAdded' : 'toast.created', { name }));
  if (!tracks.length) go('local-playlist', { id });
}

async function addTo(p, tracks) {
  await api.addToPlaylist(p.id, tracks);
  await refreshLibrary();
  toast(t('toast.addedTo', { name: p.name }));
}

async function playCollection(kind, id) {
  try {
    let tracks = [], ctx = null;
    if (kind === 'album') { const a = await api.album(id); tracks = a.tracks; ctx = { kind, id, title: a.title }; }
    else if (kind === 'playlist') { const p = await api.playlist(id); tracks = p.tracks; ctx = { kind, id, title: p.title }; }
    else if (kind === 'local') { const p = await api.localPlaylist(Number(id)); tracks = p.tracks; ctx = { kind, id, title: p.name }; }
    if (tracks.length) player.playList(tracks, 0, ctx);
  } catch (e) { toast(errText(e), 'err'); }
}

const actions = {
  like: (el) => { const r = trackFromEl(el); if (r) toggleLike(r.track); },
  menu: (el, e) => { const r = trackFromEl(el); if (r) trackMenu(r.track, r.ctx, r.idx, e.clientX, e.clientY); },
  'play-collection': (el) => playCollection(el.dataset.kind, el.dataset.id),
  'play-page': () => pageTracks.length && player.playList(pageTracks, 0, pageCtx),
  'shuffle-page': () => {
    if (!pageTracks.length) return;
    if (!player.shuffle) player.toggleShuffle();
    player.playList(pageTracks, Math.floor(Math.random() * pageTracks.length), pageCtx);
  },
  'download-page': () => downloadTracks(pageTracks),
  'save-page': async () => {
    await api.createPlaylist(pageCtx.title, pageTracks);
    await refreshLibrary();
    toast(t('toast.saved', { name: pageCtx.title }));
  },
  'rename-local': async () => {
    const name = await promptDialog(t('dlg.rename'), pageCtx.title);
    if (!name) return;
    await api.renamePlaylist(pageCtx.id, name);
    await refreshLibrary(); render();
  },
  'delete-local': async () => {
    if (!confirm(t('dlg.delete', { name: pageCtx.title }))) return;
    await api.deletePlaylist(pageCtx.id);
    await refreshLibrary();
    go('home');
  },
  'radio-artist': () => { if (pageTracks[0]) startRadio(pageTracks[0]); },
  'q-remove': (el) => { player.removeAt(Number(el.closest('.qrow').dataset.qidx)); },
  'open-dl-dir': () => api.openDownloadsDir().catch((e) => toast(errText(e), 'err')),
  'save-dl-dir': async () => {
    try { await api.setDownloadDir($('#dl-dir').value.trim()); toast(t('toast.settingsSaved')); render(); } catch (e) { toast(errText(e), 'err'); }
  },
  login: async (el) => {
    const text = $('#cookie-text').value.trim();
    if (!text) return toast(t('toast.pasteCookies'), 'err');
    el.disabled = true;
    try {
      await api.login(text);
      state.settings = await api.settings();
      renderSidebar();
      toast(t('toast.loggedIn'));
      await sync();
      render();
    } catch (e) { toast(errText(e), 'err'); } finally { el.disabled = false; }
  },
  logout: async () => { await api.logout(); state.settings = await api.settings(); state.remotePlaylists = []; renderSidebar(); render(); },
  sync: async (el) => { el.disabled = true; await sync(); el.disabled = false; },
  'install-ytdlp': async (el) => {
    el.disabled = true; el.textContent = t('set.downloading');
    try { await api.installYtdlp(); toast(t('toast.ytdlpOk')); } catch (e) { toast(errText(e), 'err'); }
    render();
  },
};

async function sync() {
  try {
    const r = await api.syncAccount();
    await refreshLibrary();
    await refreshRemote();
    toast(t('toast.synced', { tracks: tracksLabel(r.liked), n: r.playlists }));
  } catch (e) { toast(t('toast.syncFail', { e: errText(e) }), 'err'); }
}

function trackFromEl(el) {
  const row = el.closest('[data-list]');
  if (!row) return null;
  const list = state.lists.get(row.dataset.list);
  const idx = Number(row.dataset.idx);
  const tl = row.closest('.tracklist');
  let ctx = null;
  try { ctx = tl ? JSON.parse(tl.dataset.ctx) : null; } catch { /* none */ }
  return list ? { list, idx, track: list[idx], ctx } : null;
}

// ---------------------------------------------------------------- global events
document.addEventListener('click', (e) => {
  const act = e.target.closest('[data-act]');
  if (act) { e.stopPropagation(); actions[act.dataset.act]?.(act, e); return; }
  const num = e.target.closest('.trow .tnum');
  if (num) {
    const r = trackFromEl(num);
    if (r) { if (player.current?.id === r.track.id) player.toggle(); else player.playList(r.list, r.idx, r.ctx); }
    return;
  }
  const goEl = e.target.closest('[data-go]');
  if (goEl) { const id = goEl.dataset.id; go(goEl.dataset.go, id ? { id } : {}); return; }
  const navEl = e.target.closest('[data-nav]');
  if (navEl) {
    const v = navEl.dataset.nav;
    if (v === 'local-playlist') go(v, { id: navEl.dataset.pl });
    else if (v === 'playlist') go(v, { id: navEl.dataset.rpl });
    else if (v === 'search') { $('#search-input').focus(); go('search', { q: '' }); }
    else go(v);
    return;
  }
  const chip = e.target.closest('[data-search]');
  if (chip) { doSearch(chip.dataset.search); return; }
  const q = e.target.closest('.qrow');
  if (q) player.jumpTo(Number(q.dataset.qidx));
});

document.addEventListener('dblclick', (e) => {
  const row = e.target.closest('.trow');
  if (!row || e.target.closest('button, a')) return;
  const r = trackFromEl(row);
  if (r) player.playList(r.list, r.idx, r.ctx);
});

document.addEventListener('contextmenu', (e) => {
  const row = e.target.closest('.trow, .qrow');
  if (row) {
    e.preventDefault();
    const r = trackFromEl(row);
    if (r) trackMenu(r.track, r.ctx, r.idx, e.clientX, e.clientY);
    return;
  }
  if (!e.target.closest('input, textarea')) e.preventDefault();
});

$('#nav-back').onclick = () => { if (nav.pos > 0) { nav.pos--; render(); } };
$('#nav-fwd').onclick = () => { if (nav.pos < nav.stack.length - 1) { nav.pos++; render(); } };
$('#new-playlist').onclick = () => newPlaylist();
$('#rp-close').onclick = () => { $('#rightpanel').classList.add('hidden'); syncPanelBtns(); };
$$('.rp-tab').forEach((b) => (b.onclick = () => openPanel(b.dataset.tab)));
$('#btn-queue').onclick = () => openPanel('queue');
$('#btn-lyrics').onclick = () => openPanel('lyrics');

// ---------------------------------------------------------------- search box
const sInput = $('#search-input');
const suggest = $('#suggest');
function doSearch(q) {
  q = q.trim();
  if (!q) return;
  suggest.classList.add('hidden');
  sInput.value = q;
  try {
    const h = JSON.parse(localStorage.getItem('searches') || '[]').filter((x) => x !== q);
    localStorage.setItem('searches', JSON.stringify([q, ...h].slice(0, 12)));
  } catch { /* ignore */ }
  const cur = nav.stack[nav.pos];
  if (cur?.view === 'search') { nav.stack[nav.pos] = { view: 'search', params: { q }, scroll: 0 }; render(); }
  else go('search', { q });
}
const loadSuggest = debounce(async (q) => {
  if (!q || document.activeElement !== sInput) return suggest.classList.add('hidden');
  try {
    const terms = await api.suggestions(q);
    if (sInput.value.trim() !== q) return;
    suggest.innerHTML = terms.slice(0, 8).map((s) => `<div class="sg" data-search="${esc(s)}">${icon('search')}<span>${esc(s)}</span></div>`).join('');
    suggest.classList.toggle('hidden', !terms.length);
  } catch { suggest.classList.add('hidden'); }
}, 180);
sInput.addEventListener('input', () => loadSuggest(sInput.value.trim()));
sInput.addEventListener('keydown', (e) => {
  const items = $$('.sg', suggest);
  const cur = items.findIndex((x) => x.classList.contains('sel'));
  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    e.preventDefault();
    const n = items.length ? (cur + (e.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length : -1;
    items.forEach((x, i) => x.classList.toggle('sel', i === n));
    if (items[n]) sInput.value = items[n].dataset.search;
  } else if (e.key === 'Enter') doSearch(sInput.value);
  else if (e.key === 'Escape') { suggest.classList.add('hidden'); sInput.blur(); }
});
sInput.addEventListener('blur', () => setTimeout(() => suggest.classList.add('hidden'), 150));
sInput.addEventListener('focus', () => { if (sInput.value.trim()) loadSuggest(sInput.value.trim()); });

// ---------------------------------------------------------------- player bar
const audio = player.audio;
function updateNowPlaying() {
  const cur = player.current;
  $('#player').classList.toggle('empty', !cur);
  if (!cur) return;
  $('#np-cover').src = cur.cover || '';
  $('#np-title').innerHTML = cur.album?.id ? `<a class="link" data-go="album" data-id="${esc(cur.album.id)}">${esc(cur.title)}</a>` : esc(cur.title);
  $('#np-artist').innerHTML = artistLinks(cur.artists);
  const liked = state.liked.has(cur.id);
  $('#np-like').classList.toggle('on', liked);
  setIcon($('#np-like'), liked ? 'heart-fill' : 'heart');
  document.title = `${cur.title} · ${artistNames(cur.artists)}`;
}
function updateState() {
  setIcon($('#btn-play'), player.playing ? 'pause' : 'play');
  $('#btn-shuffle').classList.toggle('on', player.shuffle);
  $('#btn-repeat').classList.toggle('on', player.repeat !== 'off');
  setIcon($('#btn-repeat'), player.repeat === 'one' ? 'repeat-one' : 'repeat');
  const cur = player.current?.id;
  $$('.trow').forEach((r) => {
    r.classList.toggle('playing', r.dataset.tid === cur);
    r.classList.toggle('paused', r.dataset.tid === cur && !player.playing);
  });
  if ('mediaSession' in navigator) navigator.mediaSession.playbackState = player.playing ? 'playing' : 'paused';
}
let dragging = null;
function updateTime() {
  const d = audio.duration, c = audio.currentTime;
  $('#t-dur').textContent = fmt(isFinite(d) ? d : player.current?.duration);
  if (dragging === 'seek') return;
  $('#t-cur').textContent = fmt(c);
  const f = isFinite(d) && d > 0 ? c / d : 0;
  $('#seek-fill').style.width = `${f * 100}%`;
  $('#seek-knob').style.left = `${f * 100}%`;
  if (audio.buffered.length && isFinite(d)) $('#seek-buf').style.width = `${(audio.buffered.end(audio.buffered.length - 1) / d) * 100}%`;
  if ('mediaSession' in navigator && isFinite(d) && d > 0) {
    try { navigator.mediaSession.setPositionState({ duration: d, position: Math.min(c, d), playbackRate: 1 }); } catch { /* ignore */ }
  }
}
function updateVolume() {
  const v = audio.muted ? 0 : audio.volume;
  $('#vol-fill').style.width = `${v * 100}%`;
  $('#vol-knob').style.left = `${v * 100}%`;
  setIcon($('#btn-mute'), v === 0 ? 'volume-off' : 'volume');
}

function bindBar(el, kind, onMove, onEnd) {
  const frac = (e) => { const r = el.getBoundingClientRect(); return Math.max(0, Math.min(1, (e.clientX - r.left) / r.width)); };
  el.addEventListener('pointerdown', (e) => {
    dragging = kind;
    el.setPointerCapture(e.pointerId);
    el.classList.add('drag');
    onMove(frac(e));
    const move = (ev) => onMove(frac(ev));
    const up = (ev) => {
      el.removeEventListener('pointermove', move);
      el.removeEventListener('pointerup', up);
      el.removeEventListener('pointercancel', up);
      el.classList.remove('drag');
      dragging = null;
      onEnd?.(frac(ev));
    };
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', up);
    el.addEventListener('pointercancel', up);
  });
}
bindBar($('#seekbar'), 'seek', (f) => {
  $('#seek-fill').style.width = `${f * 100}%`;
  $('#seek-knob').style.left = `${f * 100}%`;
  if (isFinite(audio.duration)) $('#t-cur').textContent = fmt(f * audio.duration);
}, (f) => player.seek(f));
bindBar($('#volbar'), 'vol', (f) => player.setVolume(f));
$('#volbar').addEventListener('wheel', (e) => { e.preventDefault(); player.setVolume(audio.volume - Math.sign(e.deltaY) * 0.05); }, { passive: false });

$('#btn-play').onclick = () => player.toggle();
$('#btn-next').onclick = () => player.next();
$('#btn-prev').onclick = () => player.prev();
$('#btn-shuffle').onclick = () => player.toggleShuffle();
$('#btn-repeat').onclick = () => player.cycleRepeat();
$('#btn-mute').onclick = () => player.toggleMute();
$('#np-like').onclick = () => player.current && toggleLike(player.current);
$('#np-cover').onclick = () => openPanel('queue');

player.on('track', () => { updateNowPlaying(); lyricsFor = null; renderPanel(); });
player.on('queue', () => { if (rpTab === 'queue') renderPanel(); });
player.on('state', updateState);
player.on('time', updateTime);
player.on('volume', updateVolume);
player.on('error', (x) => toast(t('toast.playFail', { title: x.title }), 'err'));

// keyboard shortcuts
document.addEventListener('keydown', (e) => {
  if (e.target.closest('input, textarea, select')) return;
  if (e.code === 'Space') { e.preventDefault(); player.toggle(); }
  else if (e.code === 'ArrowRight' && e.ctrlKey) player.next();
  else if (e.code === 'ArrowLeft' && e.ctrlKey) player.prev();
  else if (e.altKey && e.key === 'ArrowLeft') $('#nav-back').click();
  else if (e.code === 'ArrowRight') audio.currentTime = Math.min(audio.duration || 0, audio.currentTime + 5);
  else if (e.code === 'ArrowLeft') audio.currentTime = Math.max(0, audio.currentTime - 5);
  else if (e.code === 'ArrowUp' && e.ctrlKey) player.setVolume(audio.volume + 0.05);
  else if (e.code === 'ArrowDown' && e.ctrlKey) player.setVolume(audio.volume - 0.05);
  else if ((e.key === 'k' && e.ctrlKey) || e.key === '/') { e.preventDefault(); sInput.focus(); sInput.select(); }
  else if (e.key === 'l' && e.altKey) { if (player.current) toggleLike(player.current); }
});
window.addEventListener('mouseup', (e) => { if (e.button === 3) $('#nav-back').click(); if (e.button === 4) $('#nav-fwd').click(); });

// ---------------------------------------------------------------- backend events
listen('download-progress', (p) => {
  state.dlProgress.set(p.id, p);
  if (p.state === 'done') {
    state.downloaded.add(p.id);
    state.dlProgress.delete(p.id);
    renderSidebar();
  }
  if (p.state === 'error') { state.dlProgress.delete(p.id); toast(t('toast.dlError', { e: p.error || '' }), 'err'); }
  $$(`.trow[data-tid="${CSS.escape(p.id)}"] .tartist`).forEach((el) => {
    const mark = el.querySelector('.dl-mark');
    const html = p.state === 'done' ? dlMarkDone() : p.state === 'error' ? '' : dlMarkBusy(p.progress);
    if (mark) mark.outerHTML = html; else el.insertAdjacentHTML('afterbegin', html);
  });
});

// ---------------------------------------------------------------- boot
(async function boot() {
  translateDom();
  hydrateIcons();
  try { player.autoplay = JSON.parse(localStorage.getItem('autoplay') ?? 'true'); } catch { /* default */ }
  await initApi();
  state.settings = await api.settings().catch(() => ({}));
  await refreshLibrary().catch((e) => console.error(e));
  refreshRemote();
  player.restore();
  updateNowPlaying();
  go('home');
})();
