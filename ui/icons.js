// Inline SVG icon set (24x24, currentColor).
const P = {
  home: '<path d="M3 10.5 12 3l9 7.5V21a1 1 0 0 1-1 1h-5v-7H9v7H4a1 1 0 0 1-1-1z"/>',
  search: '<circle cx="11" cy="11" r="7" fill="none" stroke="currentColor" stroke-width="2.2"/><path d="m20 20-4.2-4.2" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"/>',
  library: '<path d="M4 3h2v18H4zM9 3h2v18H9zM14.2 3.6l1.9-.6 5.5 17.1-1.9.6z"/>',
  plus: '<path d="M11 4h2v7h7v2h-7v7h-2v-7H4v-2h7z"/>',
  heart: '<path d="M12 20.3 10.6 19C5.4 14.3 2 11.2 2 7.5 2 4.4 4.4 2 7.5 2c1.7 0 3.4.8 4.5 2.1C13.1 2.8 14.8 2 16.5 2 19.6 2 22 4.4 22 7.5c0 3.7-3.4 6.8-8.6 11.5z" fill="none" stroke="currentColor" stroke-width="2"/>',
  'heart-fill': '<path d="M12 20.3 10.6 19C5.4 14.3 2 11.2 2 7.5 2 4.4 4.4 2 7.5 2c1.7 0 3.4.8 4.5 2.1C13.1 2.8 14.8 2 16.5 2 19.6 2 22 4.4 22 7.5c0 3.7-3.4 6.8-8.6 11.5z"/>',
  download: '<path d="M11 3h2v10.2l3.6-3.6 1.4 1.4-6 6-6-6 1.4-1.4 3.6 3.6zM4 19h16v2H4z"/>',
  downloaded: '<circle cx="12" cy="12" r="10"/><path d="M11 6h2v6.6l2.3-2.3 1.4 1.4L12 16.4l-4.7-4.7 1.4-1.4 2.3 2.3z" fill="var(--bg)"/>',
  history: '<path d="M13 3a9 9 0 1 1-8.5 12h2.1A7 7 0 1 0 6 8.4L8.6 11H2V4.4l2.6 2.6A9 9 0 0 1 13 3m-1 4h2v5.2l3.6 2.1-1 1.7L12 13.4z"/>',
  settings: '<path d="M19.4 13a7.5 7.5 0 0 0 0-2l2.1-1.6-2-3.5-2.5 1a7.4 7.4 0 0 0-1.7-1L15 3h-4l-.3 2.9a7.4 7.4 0 0 0-1.7 1l-2.5-1-2 3.5L6.6 11a7.5 7.5 0 0 0 0 2l-2.1 1.6 2 3.5 2.5-1a7.4 7.4 0 0 0 1.7 1L11 21h4l.3-2.9a7.4 7.4 0 0 0 1.7-1l2.5 1 2-3.5zM13 15.5a3.5 3.5 0 1 1 0-7 3.5 3.5 0 0 1 0 7" transform="translate(-1 0)"/>',
  'chev-left': '<path d="M15.4 5.4 14 4l-8 8 8 8 1.4-1.4L8.8 12z"/>',
  'chev-right': '<path d="M8.6 5.4 10 4l8 8-8 8-1.4-1.4 6.6-6.6z"/>',
  close: '<path d="m6.4 5 5.6 5.6L17.6 5 19 6.4 13.4 12l5.6 5.6-1.4 1.4-5.6-5.6L6.4 19 5 17.6l5.6-5.6L5 6.4z"/>',
  play: '<path d="M7 4.5v15a1 1 0 0 0 1.5.9l12-7.5a1 1 0 0 0 0-1.8l-12-7.5A1 1 0 0 0 7 4.5"/>',
  pause: '<path d="M6 4h4v16H6zM14 4h4v16h-4z"/>',
  next: '<path d="M5 5.2v13.6a.8.8 0 0 0 1.2.7l9.8-6.8a.8.8 0 0 0 0-1.4L6.2 4.5A.8.8 0 0 0 5 5.2M17 4h2v16h-2z"/>',
  prev: '<path d="M19 5.2v13.6a.8.8 0 0 1-1.2.7L8 12.7a.8.8 0 0 1 0-1.4l9.8-6.8a.8.8 0 0 1 1.2.7M5 4h2v16H5z"/>',
  shuffle: '<path d="M17 3l4 4-4 4V8h-2.3l-2.6 3.3-1.3-1.6L13.7 6H17zM3 6h4.3l9 11.5H17v-3l4 4-4 4v-3h-1.7L6.3 8H3zm0 10h3.3l2.2-2.8 1.3 1.6L7.3 18H3z"/>',
  repeat: '<path d="M7 7h10v3l4-4-4-4v3H5v6h2zm10 10H7v-3l-4 4 4 4v-3h12v-6h-2z"/>',
  'repeat-one': '<path d="M7 7h10v3l4-4-4-4v3H5v6h2zm10 10H7v-3l-4 4 4 4v-3h12v-6h-2z"/><path d="M11 9.5h1.6V15h-1.4v-3.9l-1 .4V10z"/>',
  volume: '<path d="M3 9v6h4l5 5V4L7 9zm13.5 3A4.5 4.5 0 0 0 14 8v8a4.5 4.5 0 0 0 2.5-4M14 3.2v2.1a7 7 0 0 1 0 13.4v2.1a9 9 0 0 0 0-17.6"/>',
  'volume-off': '<path d="M3 9v6h4l5 5V4L7 9zm18.4 0L20 7.6 17.4 10.2 14.8 7.6 13.4 9l2.6 2.6-2.6 2.6 1.4 1.4 2.6-2.6 2.6 2.6 1.4-1.4-2.6-2.6z"/>',
  queue: '<path d="M3 5h13v2H3zm0 4h13v2H3zm0 4h8v2H3zm12 0v7l6-3.5z"/>',
  mic: '<path d="M12 14a3 3 0 0 0 3-3V5a3 3 0 0 0-6 0v6a3 3 0 0 0 3 3m5.3-3A5.3 5.3 0 0 1 12 16.3 5.3 5.3 0 0 1 6.7 11H5a7 7 0 0 0 6 6.9V21h2v-3.1a7 7 0 0 0 6-6.9z"/>',
  more: '<circle cx="5" cy="12" r="2"/><circle cx="12" cy="12" r="2"/><circle cx="19" cy="12" r="2"/>',
  clock: '<path d="M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20m0 18a8 8 0 1 1 0-16 8 8 0 0 1 0 16m.5-13H11v6l5.2 3.2.8-1.3-4.5-2.7z"/>',
  music: '<path d="M12 3v10.6A4 4 0 1 0 14 17V7h4V3z"/>',
  user: '<path d="M12 12a5 5 0 1 0 0-10 5 5 0 0 0 0 10m0 2c-3.3 0-10 1.7-10 5v3h20v-3c0-3.3-6.7-5-10-5"/>',
  trash: '<path d="M9 3h6l1 1h4v2H4V4h4zM6 8h12l-1 13H7z"/>',
  edit: '<path d="M3 17.2V21h3.8l11-11-3.8-3.8zM20.7 7a1 1 0 0 0 0-1.4l-2.3-2.3a1 1 0 0 0-1.4 0l-1.8 1.8L19 8.8z"/>',
  radio: '<path d="M12 10a2 2 0 1 0 0 4 2 2 0 0 0 0-4M7.8 7.8a6 6 0 0 0 0 8.4L6.4 17.6a8 8 0 0 1 0-11.2zm8.4 0 1.4-1.4a8 8 0 0 1 0 11.2l-1.4-1.4a6 6 0 0 0 0-8.4M4.9 4.9a10 10 0 0 0 0 14.2l-1.4 1.4a12 12 0 0 1 0-17zm14.2 0 1.4-1.4a12 12 0 0 1 0 17l-1.4-1.4a10 10 0 0 0 0-14.2"/>',
  check: '<path d="M9 16.2 4.8 12l-1.4 1.4L9 19 21 7l-1.4-1.4z"/>',
  sync: '<path d="M12 4V1L8 5l4 4V6a6 6 0 0 1 5.7 7.9l1.5 1.5A8 8 0 0 0 12 4m0 14a6 6 0 0 1-5.7-7.9L4.8 8.6A8 8 0 0 0 12 20v3l4-4-4-4z"/>',
  spinner: '<path d="M12 2a10 10 0 1 0 10 10h-2.5A7.5 7.5 0 1 1 12 4.5z"/>',
  lock: '<path d="M7 10V7a5 5 0 0 1 10 0v3h1a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V11a1 1 0 0 1 1-1zm2 0h6V7a3 3 0 0 0-6 0z"/>',
};

export function icon(name, cls = '') {
  const body = P[name] || '';
  return `<svg class="ic ${cls}" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">${body}</svg>`;
}

export function hydrateIcons(root = document) {
  root.querySelectorAll('i[data-icon]').forEach((el) => {
    el.outerHTML = icon(el.dataset.icon);
  });
}

export function setIcon(btn, name) {
  const svg = btn.querySelector('svg');
  if (svg) svg.outerHTML = icon(name);
  else btn.insertAdjacentHTML('afterbegin', icon(name));
}
