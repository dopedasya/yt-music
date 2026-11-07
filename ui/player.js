// Playback engine: queue, shuffle/repeat, autoplay radio, media session.
import { api, streamUrl } from './api.js';

const audio = document.getElementById('audio');
const listeners = {};
const emit = (ev, data) => (listeners[ev] || []).forEach((f) => f(data));

function shuffled(arr) {
  const a = arr.slice();
  for (let i = a.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [a[i], a[j]] = [a[j], a[i]];
  }
  return a;
}

const store = {
  get(k, d) { try { const v = localStorage.getItem(k); return v == null ? d : JSON.parse(v); } catch { return d; } },
  set(k, v) { try { localStorage.setItem(k, JSON.stringify(v)); } catch { /* ignore */ } },
};

export const player = {
  queue: [],
  original: null, // unshuffled order when shuffle is on
  index: -1,
  shuffle: false,
  repeat: 'off', // off | all | one
  autoplay: true,
  context: null, // {kind, id, title} - where the queue came from
  failures: 0,
  loadingRadio: false,

  on(ev, fn) { (listeners[ev] ||= []).push(fn); },
  get current() { return this.queue[this.index] || null; },
  get playing() { return !audio.paused && !audio.ended; },
  get audio() { return audio; },

  playList(tracks, start = 0, context = null) {
    if (!tracks.length) return;
    this.context = context;
    const first = tracks[start];
    if (this.shuffle) {
      this.original = tracks.slice();
      const rest = shuffled(tracks.filter((_, i) => i !== start));
      this.queue = [first, ...rest];
      this.index = 0;
    } else {
      this.original = null;
      this.queue = tracks.slice();
      this.index = start;
    }
    this._load(true);
  },

  playTrack(track) { this.playList([track], 0, null); },

  playNext(tracks) {
    if (!this.current) return this.playList(tracks, 0);
    this.queue.splice(this.index + 1, 0, ...tracks);
    if (this.original) this.original.push(...tracks);
    this._changed();
  },

  addToQueue(tracks) {
    if (!this.current) return this.playList(tracks, 0);
    this.queue.push(...tracks);
    if (this.original) this.original.push(...tracks);
    this._changed();
  },

  removeAt(i) {
    if (i === this.index) return;
    this.queue.splice(i, 1);
    if (i < this.index) this.index--;
    this._changed();
  },

  jumpTo(i) {
    if (i < 0 || i >= this.queue.length) return;
    this.index = i;
    this._load(true);
  },

  toggle() {
    if (!this.current) return;
    if (audio.paused) audio.play().catch(() => {});
    else audio.pause();
  },

  async next(auto = false) {
    if (!this.queue.length) return;
    if (auto && this.repeat === 'one') { audio.currentTime = 0; audio.play().catch(() => {}); return; }
    if (this.index < this.queue.length - 1) {
      this.index++;
      return this._load(true);
    }
    if (this.repeat === 'all') { this.index = 0; return this._load(true); }
    if (this.autoplay && await this._extendWithRadio()) {
      this.index++;
      return this._load(true);
    }
    if (auto) { audio.pause(); emit('state'); }
  },

  prev() {
    if (audio.currentTime > 3 || this.index <= 0) { audio.currentTime = 0; return; }
    this.index--;
    this._load(true);
  },

  seek(frac) {
    if (!isFinite(audio.duration)) return;
    audio.currentTime = Math.max(0, Math.min(1, frac)) * audio.duration;
  },

  setVolume(v) {
    audio.volume = Math.max(0, Math.min(1, v));
    audio.muted = false;
    store.set('volume', audio.volume);
    emit('volume');
  },

  toggleMute() { audio.muted = !audio.muted; emit('volume'); },

  toggleShuffle() {
    this.shuffle = !this.shuffle;
    const cur = this.current;
    if (this.shuffle) {
      this.original = this.queue.slice();
      const upcoming = shuffled(this.queue.slice(this.index + 1));
      this.queue = [...this.queue.slice(0, this.index + 1), ...upcoming];
    } else if (this.original) {
      this.queue = this.original;
      this.original = null;
      this.index = Math.max(0, this.queue.findIndex((t) => t === cur));
    }
    store.set('shuffle', this.shuffle);
    this._changed();
  },

  cycleRepeat() {
    this.repeat = { off: 'all', all: 'one', one: 'off' }[this.repeat];
    store.set('repeat', this.repeat);
    emit('state');
  },

  async _extendWithRadio() {
    const seed = this.current;
    if (!seed || this.loadingRadio) return false;
    this.loadingRadio = true;
    try {
      const have = new Set(this.queue.map((t) => t.id));
      const more = (await api.radio(seed.id)).filter((t) => !have.has(t.id));
      if (!more.length) return false;
      this.queue.push(...more.slice(0, 25));
      this._changed();
      return true;
    } catch (e) {
      console.warn('radio failed', e);
      return false;
    } finally {
      this.loadingRadio = false;
    }
  },

  _load(autoplay) {
    const t = this.current;
    if (!t) return;
    audio.src = streamUrl(t.id);
    if (autoplay) audio.play().catch((e) => console.warn('play()', e));
    this._updateSession();
    this._changed();
    emit('track', t);
    api.recordPlay(t).catch(() => {});
    // Warm up the stream URL of the next track so skipping is instant.
    const nxt = this.queue[this.index + 1];
    if (nxt) setTimeout(() => api.prefetch(nxt.id).catch(() => {}), 3000);
    // Refill the queue in the background when approaching its end.
    if (this.autoplay && this.repeat === 'off' && this.index >= this.queue.length - 2) {
      this._extendWithRadio();
    }
  },

  _changed() {
    store.set('queue', { queue: this.queue.slice(0, 500), index: this.index, context: this.context });
    emit('queue');
    emit('state');
  },

  _updateSession() {
    if (!('mediaSession' in navigator)) return;
    const t = this.current;
    navigator.mediaSession.metadata = new MediaMetadata({
      title: t.title,
      artist: (t.artists || []).map((a) => a.name).join(', '),
      album: t.album?.name || '',
      artwork: t.cover ? [{ src: t.cover, sizes: '544x544', type: 'image/jpeg' }] : [],
    });
  },

  restore() {
    audio.volume = store.get('volume', 0.8);
    this.shuffle = store.get('shuffle', false);
    this.repeat = store.get('repeat', 'off');
    const saved = store.get('queue', null);
    if (saved && saved.queue?.length) {
      this.queue = saved.queue;
      this.index = Math.min(saved.index, saved.queue.length - 1);
      this.context = saved.context;
      const t = this.current;
      if (t) {
        audio.src = streamUrl(t.id);
        audio.preload = 'metadata';
        this._updateSession();
        emit('track', t);
      }
    }
    emit('queue');
    emit('state');
    emit('volume');
  },
};

audio.addEventListener('play', () => emit('state'));
audio.addEventListener('pause', () => emit('state'));
audio.addEventListener('playing', () => { player.failures = 0; emit('state'); });
audio.addEventListener('timeupdate', () => emit('time'));
audio.addEventListener('progress', () => emit('time'));
audio.addEventListener('durationchange', () => emit('time'));
audio.addEventListener('ended', () => player.next(true));
audio.addEventListener('error', () => {
  const t = player.current;
  if (!t || !audio.src) return;
  player.failures++;
  emit('error', t);
  // Skip broken tracks, but don't spin forever if everything fails (e.g. offline).
  if (player.failures < 4) setTimeout(() => player.next(true), 800);
});

if ('mediaSession' in navigator) {
  const ms = navigator.mediaSession;
  ms.setActionHandler('play', () => audio.play());
  ms.setActionHandler('pause', () => audio.pause());
  ms.setActionHandler('nexttrack', () => player.next());
  ms.setActionHandler('previoustrack', () => player.prev());
  try { ms.setActionHandler('seekto', (d) => { audio.currentTime = d.seekTime; }); } catch { /* unsupported */ }
}
