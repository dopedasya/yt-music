// Thin wrapper over Tauri commands. Every backend call goes through here.
const T = window.__TAURI__;
const invoke = (cmd, args) => T.core.invoke(cmd, args);
export const listen = (ev, cb) => T.event.listen(ev, (e) => cb(e.payload));

let base = '';
export async function initApi() {
  base = await invoke('stream_base');
}
export const streamUrl = (id) => `${base}/stream/${encodeURIComponent(id)}`;

export const api = {
  // YouTube Music
  search: (query) => invoke('search', { query }),
  suggestions: (query) => invoke('search_suggestions', { query }),
  home: () => invoke('home'),
  album: (id) => invoke('get_album', { id }),
  artist: (id) => invoke('get_artist', { id }),
  playlist: (id) => invoke('get_playlist', { id }),
  radio: (id) => invoke('get_radio', { id }),
  lyrics: (id) => invoke('get_lyrics', { id }),
  prefetch: (id) => invoke('prefetch_stream', { id }),

  // Library
  likedTracks: () => invoke('liked_tracks'),
  likedIds: () => invoke('liked_ids'),
  setLiked: (track, liked) => invoke('set_liked', { track, liked }),
  playlists: () => invoke('list_playlists'),
  localPlaylist: (id) => invoke('get_local_playlist', { id }),
  createPlaylist: (name, tracks = []) => invoke('create_playlist', { name, tracks }),
  renamePlaylist: (id, name) => invoke('rename_playlist', { id, name }),
  deletePlaylist: (id) => invoke('delete_playlist', { id }),
  addToPlaylist: (id, tracks) => invoke('add_to_playlist', { id, tracks }),
  removeFromPlaylist: (id, pos) => invoke('remove_from_playlist', { id, pos }),
  recordPlay: (track) => invoke('record_play', { track }),
  history: () => invoke('get_history'),

  // Downloads
  download: (tracks) => invoke('download_tracks', { tracks }),
  downloads: () => invoke('list_downloads'),
  downloadedIds: () => invoke('downloaded_ids'),
  deleteDownload: (id) => invoke('delete_download', { id }),
  openDownloadsDir: () => invoke('open_downloads_dir'),

  // Account & settings
  settings: () => invoke('get_settings'),
  setDownloadDir: (path) => invoke('set_download_dir', { path }),
  login: (text) => invoke('login_cookies', { text }),
  logout: () => invoke('logout'),
  syncAccount: () => invoke('sync_account'),
  remotePlaylists: () => invoke('remote_playlists'),
  installYtdlp: () => invoke('install_ytdlp'),
};
