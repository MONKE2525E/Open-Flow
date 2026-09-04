// SVG path data for all icons — 24x24 viewBox, stroke-based.
// WARNING: several icons are looked up DYNAMICALLY via `icons[entry.icon]`
// (Sidebar nav entries, settingsSections.ts) — a static grep for `icons.foo`
// does NOT prove an icon is unused. Keep this set in sync with the literal
// `icon:` keys in Sidebar.svelte and settingsSections.ts.
export const icons = {
  home:     `<path d="M3 11l9-8 9 8"/><path d="M5 10v10h14V10"/>`,
  book:     `<path d="M4 5a2 2 0 0 1 2-2h12v18H6a2 2 0 0 1-2-2z"/><path d="M8 7h8M8 11h6"/>`,
  scissors: `<circle cx="6" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><path d="M20 4 8.12 15.88M14.47 14.48 20 20M8.12 8.12 12 12"/>`,
  type:     `<path d="M4 6V4h16v2"/><path d="M9 20h6M12 4v16"/>`,
  settings: `<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.6 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.6a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>`,
  help:     `<circle cx="12" cy="12" r="10"/><path d="M9.25 9.25a2.75 2.75 0 0 1 5.5.5c0 1.9-2.75 2.8-2.75 4.25"/><path d="M12 17.5v.01"/><circle cx="12" cy="17.5" r="0.9" fill="currentColor" stroke="none"/>`,
  bell:     `<path d="M18 8a6 6 0 0 0-12 0c0 7-3 9-3 9h18s-3-2-3-9"/><path d="M13.73 21a2 2 0 0 1-3.46 0"/>`,
  min:      `<path d="M5 12h14"/>`,
  close:    `<path d="M6 6l12 12M6 18 18 6"/>`,
  key:      `<circle cx="7.5" cy="15.5" r="3.5"/><path d="m21 2-9.6 9.6M15 6l3 3"/>`,
  chart:    `<path d="M4 20V4"/><path d="M4 20h16"/><path d="M8 20v-6M13 20V8M18 20v-9"/>`,
  sliders:  `<path d="M4 21v-7M4 10V3M12 21v-9M12 8V3M20 21v-5M20 12V3M1 14h6M9 8h6M17 16h6"/>`,
  command:  `<path d="M18 3a3 3 0 0 0-3 3v12a3 3 0 0 0 3 3 3 3 0 0 0 3-3 3 3 0 0 0-3-3H6a3 3 0 0 0-3 3 3 3 0 0 0 3 3 3 3 0 0 0 3-3V6a3 3 0 0 0-3-3 3 3 0 0 0-3 3 3 3 0 0 0 3 3h12a3 3 0 0 0 3-3 3 3 0 0 0-3-3z"/>`,
  lock:     `<rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/>`,
  apps:     `<rect x="3" y="4" width="18" height="12" rx="2"/><path d="M8 20h8M12 16v4"/>`,
  copy:     `<rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/>`,
  check:    `<polyline points="20 6 9 17 4 12"/>`,
  mic:      `<path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><line x1="12" y1="19" x2="12" y2="22"/><line x1="8" y1="22" x2="16" y2="22"/>`,
  shield:   `<path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/><path d="M9 12l2 2 4-4"/>`,
  code:     `<polyline points="9 8 4 12.5 9 17"/><polyline points="15 8 20 12.5 15 17"/>`,
  browser:  `<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M3 9h18"/><circle cx="6.5" cy="6.5" r="0.6" fill="currentColor" stroke="none"/><circle cx="9" cy="6.5" r="0.6" fill="currentColor" stroke="none"/>`,
  chat:     `<path d="M4 4h16v12H8l-4 4V4z"/>`,
  pencil:   `<path d="M12 20h9"/><path d="M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z"/>`,
  refresh:  `<path d="M21 12a9 9 0 1 1-2.64-6.36"/><polyline points="21 3 21 9 15 9"/>`,
  devices:  `<rect x="2" y="6" width="13" height="10" rx="2"/><path d="M17 9h3a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2h-5"/><path d="M6 20h5"/><path d="M15.5 15.5h.01"/>`,
};
