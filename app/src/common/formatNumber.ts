export function formatInteger(value: number): string {
  return new Intl.NumberFormat().format(value);
}

export function formatPercent(
  value: number,
  maximumFractionDigits = 0,
): string {
  return `${new Intl.NumberFormat(undefined, {
    maximumFractionDigits,
  }).format(value)}%`;
}

export function formatRatio(
  numerator: number,
  denominator: number,
  fractionDigits: number,
  suffix: string,
): string | null {
  return denominator > 0
    ? `${(numerator / denominator).toFixed(fractionDigits)}${suffix}`
    : null;
}
