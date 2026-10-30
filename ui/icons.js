// Small SVG icons used across the pages.

/* ---------- Icons ---------- */
const ICONS = {
  all: '<rect x="3" y="3" width="8" height="8" rx="2"/><rect x="13" y="3" width="8" height="8" rx="2"/><rect x="3" y="13" width="8" height="8" rx="2"/><rect x="13" y="13" width="8" height="8" rx="2"/>',
  player: '<circle cx="12" cy="7" r="4"/><path d="M4 21c0-4.4 3.6-7 8-7s8 2.6 8 7z"/>',
  physics: '<circle cx="14" cy="4" r="2.5"/><path d="M8 21l3-6-3-3 4-5 3 4h4v2h-5l-1.5-1.8L10 13l3 3-2.5 5z"/>',
  inventory: '<path d="M3 7l9-4 9 4v10l-9 4-9-4z" opacity=".45"/><path d="M3 7l9 4 9-4M12 11v10" fill="none" stroke="currentColor" stroke-width="2"/>',
  building: '<rect x="3" y="12" width="8" height="9" rx="1.5"/><rect x="13" y="12" width="8" height="9" rx="1.5"/><rect x="8" y="3" width="8" height="8" rx="1.5"/>',
  resources: '<path d="M12 2s7 7.5 7 12.5A7 7 0 0 1 5 14.5C5 9.5 12 2 12 2z"/>',
  weapons: '<circle cx="12" cy="12" r="8" fill="none" stroke="currentColor" stroke-width="2.4"/><circle cx="12" cy="12" r="3"/>',
  vessel: '<path d="M12 2c4 3 5 8 5 12l-2 3H9l-2-3c0-4 1-9 5-12z"/><path d="M9 18l-1 4h8l-1-4z" opacity=".5"/>',
  career: '<path d="M7 3h10v6a5 5 0 0 1-10 0z"/><path d="M10 15h4v3h3v3H7v-3h3z"/>',
  time: '<circle cx="12" cy="12" r="9" opacity=".4"/><path d="M11 6h2v6l4 2-1 2-5-3z"/>',
  crafting: '<path d="M14 3l7 7-3 3-2-2-8 8a2 2 0 0 1-3-3l8-8-2-2z"/>',
  truck: '<path d="M2 6h12v10H2zM14 9h4l4 4v3h-8z"/><circle cx="6" cy="18" r="2"/><circle cx="18" cy="18" r="2"/>',
  dot: '<circle cx="12" cy="12" r="5"/>',
};
ICONS.world = ICONS.resources;
ICONS.pinned = '<path d="M15 3l6 6-3 1-4 4 1 5-2 2-4-4-5 5H3v-1l5-5-4-4 2-2 5 1 4-4z"/>';
const icon = (k, s = 16) => `<svg width="${s}" height="${s}" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">${ICONS[k] || ICONS.dot}</svg>`;
