import type { CSSProperties, ReactNode } from "react";

import type { ParquetByteCoverage } from "../../../../../parquet/byteCoverage";
import type { ByteRange } from "../../../../../parquet/fileLayout";

export interface VisualByteBlock {
  id: string;
  byteRange: ByteRange;
  label: ReactNode;
  className?: string;
  elementId?: string;
  onClick?: () => void;
}

export function ProportionalByteBlockMap({
  blocks,
  coverage,
  ariaLabel,
}: {
  blocks: readonly VisualByteBlock[];
  coverage: ParquetByteCoverage;
  ariaLabel: string;
}) {
  return (
    <div className="file-structure-block-map" role="group" aria-label={ariaLabel}>
      {blocks.map((block) => {
        const byteLength = block.byteRange.end - block.byteRange.start;
        const fraction = coverage.coverageFraction(block.byteRange);
        const style = {
          "--file-block-weight": Math.max(byteLength, 1),
          "--file-block-loaded": `${fraction * 100}%`,
        } as CSSProperties;
        const className = `file-structure-block ${coverage.state(block.byteRange)}${
          block.className ? ` ${block.className}` : ""
        }`;

        return block.onClick ? (
          <button
            className={className}
            id={block.elementId}
            key={block.id}
            onClick={block.onClick}
            style={style}
            type="button"
          >
            {block.label}
          </button>
        ) : (
          <div
            className={className}
            id={block.elementId}
            key={block.id}
            style={style}
          >
            {block.label}
          </div>
        );
      })}
    </div>
  );
}
