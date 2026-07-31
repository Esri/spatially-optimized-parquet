export function formatParquetFileName(value: string): string {
  const path = value.split(/[?#]/, 1)[0].replaceAll("\\", "/");
  return path.slice(path.lastIndexOf("/") + 1) || value;
}
