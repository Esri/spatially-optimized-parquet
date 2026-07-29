export function formatByteSize(byteSize: number): string {
  if (byteSize < 1024) {
    return `${byteSize} B`;
  }

  const unitIndex = Math.min(Math.floor(Math.log(byteSize) / Math.log(1024)), 3);
  const units = ["B", "KB", "MB", "GB"];
  const value = byteSize / 1024 ** unitIndex;

  return `${value >= 10 ? value.toFixed(0) : value.toFixed(1)} ${units[unitIndex]}`;
}
