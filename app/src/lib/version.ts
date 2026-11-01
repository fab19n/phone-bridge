/** Formats the backend's version info for display. */
export function formatVersion(app: string, scrcpy: string): string {
  return `v${app} · scrcpy-server ${scrcpy}`;
}
