import type {
  DownloadBlockSnapshot,
  DownloadSummarySnapshot,
  DownloadTopologySnapshot,
  DownloadTrackSnapshot,
  ReadonlyExternalStore,
} from "./types";

const updateIntervalMs = 64;

interface SnapshotProvider {
  createTopologySnapshot(): DownloadTopologySnapshot;
  createSummarySnapshot(): DownloadSummarySnapshot;
  createBlockSnapshot(blockId: string): DownloadBlockSnapshot;
  createTrackSnapshot(trackId: string): DownloadTrackSnapshot;
}

class ExternalStoreChannel<Snapshot> implements ReadonlyExternalStore<Snapshot> {
  private readonly listeners = new Set<() => void>();

  constructor(private readonly readSnapshot: () => Snapshot) {}

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  getSnapshot = (): Snapshot => {
    return this.readSnapshot();
  };

  notify(): void {
    for (const listener of this.listeners) {
      listener();
    }
  }
}

class KeyedExternalStoreChannel<Key, Snapshot> {
  private readonly channels = new Map<Key, ExternalStoreChannel<Snapshot>>();

  constructor(
    private readonly readSnapshot: (key: Key) => Snapshot,
  ) {}

  channel(key: Key): ReadonlyExternalStore<Snapshot> {
    const existing = this.channels.get(key);
    if (existing) {
      return existing;
    }
    const channel = new ExternalStoreChannel(() => this.readSnapshot(key));
    this.channels.set(key, channel);
    return channel;
  }

  notify(key: Key): void {
    this.channels.get(key)?.notify();
  }
}

export class DownloadSessionPublisher {
  private topologySnapshot: DownloadTopologySnapshot;
  private summarySnapshot: DownloadSummarySnapshot;
  private readonly blockSnapshots = new Map<string, DownloadBlockSnapshot>();
  private readonly trackSnapshots = new Map<string, DownloadTrackSnapshot>();
  private readonly topologyChannel: ExternalStoreChannel<DownloadTopologySnapshot>;
  private readonly summaryChannel: ExternalStoreChannel<DownloadSummarySnapshot>;
  private readonly blockChannel: KeyedExternalStoreChannel<
    string,
    DownloadBlockSnapshot
  >;
  private readonly trackChannel: KeyedExternalStoreChannel<
    string,
    DownloadTrackSnapshot
  >;
  private readonly dirtyBlockIds = new Set<string>();
  private readonly dirtyTrackIds = new Set<string>();
  private summaryDirty = false;
  private topologyDirty = false;
  private flushTimer: ReturnType<typeof setTimeout> | null = null;
  private lastFlushTime = Number.NEGATIVE_INFINITY;

  readonly topology: ReadonlyExternalStore<DownloadTopologySnapshot>;
  readonly summary: ReadonlyExternalStore<DownloadSummarySnapshot>;

  constructor(
    private readonly snapshotProvider: SnapshotProvider,
    private readonly emptyBlockSnapshot: DownloadBlockSnapshot,
    private readonly emptyTrackSnapshot: DownloadTrackSnapshot,
  ) {
    this.topologySnapshot = snapshotProvider.createTopologySnapshot();
    this.summarySnapshot = snapshotProvider.createSummarySnapshot();
    this.topologyChannel = new ExternalStoreChannel(() => this.topologySnapshot);
    this.summaryChannel = new ExternalStoreChannel(() => this.summarySnapshot);
    this.blockChannel = new KeyedExternalStoreChannel(
      (blockId) => this.blockSnapshots.get(blockId) ?? this.emptyBlockSnapshot,
    );
    this.trackChannel = new KeyedExternalStoreChannel(
      (trackId) => this.trackSnapshots.get(trackId) ?? this.emptyTrackSnapshot,
    );
    this.topology = this.topologyChannel;
    this.summary = this.summaryChannel;
  }

  block(blockId: string): ReadonlyExternalStore<DownloadBlockSnapshot> {
    return this.blockChannel.channel(blockId);
  }

  track(trackId: string): ReadonlyExternalStore<DownloadTrackSnapshot> {
    return this.trackChannel.channel(trackId);
  }

  resetSnapshots(): void {
    this.cancelFlush();
    this.blockSnapshots.clear();
    this.trackSnapshots.clear();
    this.dirtyBlockIds.clear();
    this.dirtyTrackIds.clear();
    this.summaryDirty = false;
    this.topologyDirty = false;
  }

  markBlocks(blockIds: Iterable<string>): void {
    for (const blockId of blockIds) {
      this.dirtyBlockIds.add(blockId);
    }
  }

  markTracks(trackIds: Iterable<string>): void {
    for (const trackId of trackIds) {
      this.dirtyTrackIds.add(trackId);
    }
  }

  markSummaryDirty(): void {
    this.summaryDirty = true;
  }

  markTopologyDirty(): void {
    this.topologyDirty = true;
  }

  schedule(): void {
    if (this.flushTimer !== null) {
      return;
    }
    const delay = Math.max(0, updateIntervalMs - (Date.now() - this.lastFlushTime));
    if (delay === 0) {
      this.flush();
      return;
    }
    this.flushTimer = setTimeout(() => {
      this.flushTimer = null;
      this.flush();
    }, delay);
  }

  flush(force = false): void {
    if (
      !force &&
      !this.topologyDirty &&
      !this.summaryDirty &&
      this.dirtyBlockIds.size === 0 &&
      this.dirtyTrackIds.size === 0
    ) {
      return;
    }
    this.cancelFlush();
    this.lastFlushTime = Date.now();

    for (const blockId of this.dirtyBlockIds) {
      this.blockSnapshots.set(
        blockId,
        this.snapshotProvider.createBlockSnapshot(blockId),
      );
    }
    for (const trackId of this.dirtyTrackIds) {
      this.trackSnapshots.set(
        trackId,
        this.snapshotProvider.createTrackSnapshot(trackId),
      );
    }
    if (this.summaryDirty) {
      this.summarySnapshot = this.snapshotProvider.createSummarySnapshot();
    }
    if (this.topologyDirty) {
      this.topologySnapshot = this.snapshotProvider.createTopologySnapshot();
    }

    const dirtyBlockIds = [...this.dirtyBlockIds];
    const dirtyTrackIds = [...this.dirtyTrackIds];
    const notifySummary = this.summaryDirty;
    const notifyTopology = this.topologyDirty;
    this.dirtyBlockIds.clear();
    this.dirtyTrackIds.clear();
    this.summaryDirty = false;
    this.topologyDirty = false;

    if (notifyTopology) {
      this.topologyChannel.notify();
    }
    if (notifySummary) {
      this.summaryChannel.notify();
    }
    for (const blockId of dirtyBlockIds) {
      this.blockChannel.notify(blockId);
    }
    for (const trackId of dirtyTrackIds) {
      this.trackChannel.notify(trackId);
    }
  }

  private cancelFlush(): void {
    if (this.flushTimer !== null) {
      clearTimeout(this.flushTimer);
      this.flushTimer = null;
    }
  }
}
