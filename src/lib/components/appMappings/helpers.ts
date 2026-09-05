import { cleanAppName, normalizeExe, type InstalledApp } from '../../appMappings';

export function customExeFromSearch(search: string): string {
  return normalizeExe(search).replace(/\.exe$/, '') + '.exe';
}

export function matchesAppSearch(app: InstalledApp, search: string) {
  const query = search.trim().toLowerCase();
  if (!query) return true;

  const appName = cleanAppName(app.name || app.exe).toLowerCase();
  const appExe = normalizeExe(app.exe);
  const compactQuery = query.replace(/[^a-z0-9]/g, '');
  const compactName = appName.replace(/[^a-z0-9]/g, '');
  const compactExe = appExe.replace(/[^a-z0-9]/g, '');

  return appName.includes(query)
    || appExe.includes(query)
    || (compactQuery.length > 0 && (compactName.includes(compactQuery) || compactExe.includes(compactQuery)));
}
