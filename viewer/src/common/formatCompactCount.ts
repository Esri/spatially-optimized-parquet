export function formatCompactCount(count: number): string {
  return new Intl.NumberFormat(undefined, {
    maximumFractionDigits: 1,
    notation: "compact",
  }).format(count);
}
