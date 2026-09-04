import type { icons } from './icons';

export type SettingsSectionId =
  | 'general'
  | 'apps'
  | 'keys'
  | 'models'
  | 'privacy'
  | 'sync'
  | 'advanced'
  | 'permissions'
  | 'developer'
  | 'about';

interface SettingsSection {
  id: SettingsSectionId;
  /** Rail label. Smoke tests match sections by this exact string. */
  label: string;
  icon: keyof typeof icons;
  group: string;
  /** Only render on macOS. */
  macOnly?: boolean;
  /** Only render once Developer mode is unlocked. */
  devOnly?: boolean;
  /** Only render when Legacy features are enabled. */
  legacyOnly?: boolean;
  /** Only render when the LAN Sync beta is enabled. */
  syncOnly?: boolean;
}

/**
 * Single source of truth for the settings rail: order, labels, icons, grouping.
 * Consumed by Sidebar (renders the rail) and Settings (renders the panel), and
 * re-exported through motion.ts as SETTINGS_SECTION_ORDER for swap direction.
 *
 * Note `advanced` is labelled "Microphone" — the id predates the rename and is
 * persisted in the deep-link event payload, so the mismatch is intentional.
 */
const SETTINGS_SECTIONS: readonly SettingsSection[] = [
  { id: 'general',     label: 'General',      icon: 'sliders', group: 'Settings' },
  { id: 'apps',        label: 'App Mappings', icon: 'apps',    group: 'Settings', legacyOnly: true },
  { id: 'keys',        label: 'API Keys',     icon: 'key',     group: 'Settings' },
  { id: 'models',      label: 'Models',       icon: 'command', group: 'Settings' },
  { id: 'privacy',     label: 'Privacy',      icon: 'lock',    group: 'Settings' },
  { id: 'sync',        label: 'Sync',         icon: 'devices', group: 'Settings', syncOnly: true },
  { id: 'advanced',    label: 'Audio',        icon: 'mic',     group: 'Settings' },
  { id: 'permissions', label: 'Permissions',  icon: 'shield',  group: 'Settings', macOnly: true },
  { id: 'developer',   label: 'Developer',    icon: 'command', group: 'Settings', devOnly: true },
  { id: 'about',       label: 'About',        icon: 'help',    group: 'Verenu' },
];

/** Full id order, including gated sections, so swap direction stays stable. */
export const SETTINGS_SECTION_ORDER: readonly SettingsSectionId[] =
  SETTINGS_SECTIONS.map((s) => s.id);

export function isSettingsSectionId(value: string): value is SettingsSectionId {
  return (SETTINGS_SECTION_ORDER as readonly string[]).includes(value);
}

interface SettingsSectionGroup {
  group: string;
  items: SettingsSection[];
}

/**
 * Gated sections are omitted entirely rather than hidden with CSS — the
 * dev-mode smoke test asserts the Developer rail item has count 0 before unlock.
 */
export function visibleSettingsSections(opts: {
  isMac: boolean;
  devMode: boolean;
  legacyMode?: boolean;
  syncEnabled?: boolean;
}): SettingsSectionGroup[] {
  const groups: SettingsSectionGroup[] = [];
  for (const section of SETTINGS_SECTIONS) {
    if (section.macOnly && !opts.isMac) continue;
    if (section.devOnly && !opts.devMode) continue;
    if (section.legacyOnly && !opts.legacyMode) continue;
    if (section.syncOnly && !opts.syncEnabled) continue;
    const last = groups[groups.length - 1];
    if (last && last.group === section.group) last.items.push(section);
    else groups.push({ group: section.group, items: [section] });
  }
  return groups;
}
