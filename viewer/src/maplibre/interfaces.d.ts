export type DatasetLayerStatus =
  | { type: "idle" }
  | { type: "loading" }
  | {
      type: "ready";
      featureCount: number;
      lod: number;
      featureLimitReached: boolean;
      compression: string | null;
    }
  | { type: "failed"; message: string };
