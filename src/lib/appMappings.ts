export interface InstalledApp {
  name: string;
  exe: string;
}

export interface AppMapping {
  exe: string;
  profile: string;
  name?: string;
}

export const profileOptions = [
  { id: 'casual', label: 'Casual' },
  { id: 'formal', label: 'Formal' },
  { id: 'very_casual', label: 'Very Casual' },
] as const;

export function normalizeExe(exe: string) {
  return exe.trim().toLowerCase();
}

export function getProfileLabel(profile: string) {
  return profileOptions.find((option) => option.id === profile)?.label ?? titleize(profile);
}

export function getAppDisplayName(
  mapping: Pick<AppMapping, 'exe' | 'name'>,
  installedApps: InstalledApp[] = [],
) {
  const exe = normalizeExe(mapping.exe);
  const installed = installedApps.find((app) => normalizeExe(app.exe) === exe);
  return cleanAppName(installed?.name || mapping.name || exe);
}

export function cleanAppName(name: string) {
  const trimmed = name.trim();
  if (!trimmed) return 'Unknown App';

  return titleize(
    trimmed
      .replace(/\.exe$/i, '')
      .replace(/[_-]+/g, ' ')
      .replace(/([a-z])([A-Z])/g, '$1 $2')
      .replace(/\s+/g, ' '),
  );
}

function titleize(value: string) {
  return value
    .split(/[\s_]+/)
    .filter(Boolean)
    .map((part) => {
      const lower = part.toLowerCase();
      if (lower === 'ai') return 'AI';
      if (lower === 'api') return 'API';
      if (lower === 'ui') return 'UI';
      if (lower === 'ide') return 'IDE';
      if (lower === 'vs') return 'VS';
      return lower.charAt(0).toUpperCase() + lower.slice(1);
    })
    .join(' ');
}
