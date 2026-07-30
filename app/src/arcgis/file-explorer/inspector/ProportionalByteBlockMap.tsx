import type { CSSProperties, ReactNode } from "react";

import type { ParquetByteCoverage } from "../../../parquet/ParquetByteCoverage";
import type { ByteRange } from "../../../parquet/fileLayout";
import styles from "./InspectorDialog.module.css";

export interface VisualByteBlock {
  id: string;
  byteRange: ByteRange;
  label: ReactNode;
  className?: string;
  elementId?: string;
  onClick?: () => void;
}

/**
 * Renders byte ranges with widths proportional to their file size and fills that reflect download coverage.
 * This shared view keeps the inspector's row-group and page diagrams visually consistent.
 */
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
    <div
      className={styles.fileStructureBlockMap}
      role="group"
      aria-label={ariaLabel}
    >
      {blocks.map((block) => {
        const byteLength = block.byteRange.end - block.byteRange.start;
        const fraction = coverage.coverageFraction(block.byteRange);
        const style = {
          "--file-block-weight": Math.max(byteLength, 1),
          "--file-block-loaded": `${fraction * 100}%`,
        } as CSSProperties;
        const className = [
          styles.fileStructureBlock,
          styles[coverage.state(block.byteRange)],
          block.className,
        ].filter(Boolean).join(" ");

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
