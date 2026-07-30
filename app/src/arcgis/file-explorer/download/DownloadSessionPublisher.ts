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

/**
 * Provides a React-compatible external store around one snapshot reader.
 * It owns subscriber notification so the session model does not depend on component lifecycle details.
 */
class ExternalStoreChannel<Snapshot> implements ReadonlyExternalStore<Snapshot> {
  private readonly _listeners = new Set<() => void>();

  constructor(private readonly _readSnapshot: () => Snapshot) {}

  subscribe = (listener: () => void): (() => void) => {
    this._listeners.add(listener);
    return () => this._listeners.delete(listener);
  };

  getSnapshot = (): Snapshot => {
    return this._readSnapshot();
  };

  notify(): void {
    for (const listener of this._listeners) {
      listener();
    }
  }
}

/**
 * Provides lazily created external-store channels for independently addressable snapshots.
 * It owns the channel registry so updates notify only subscribers for the affected key.
 */
class KeyedExternalStoreChannel<Key, Snapshot> {
  private readonly _channels = new Map<Key, ExternalStoreChannel<Snapshot>>();

  constructor(
    private readonly _readSnapshot: (key: Key) => Snapshot,
  ) {}

  channel(key: Key): ReadonlyExternalStore<Snapshot> {
    const existing = this._channels.get(key);
    if (existing) {
      return existing;
    }
    const channel = new ExternalStoreChannel(() => this._readSnapshot(key));
    this._channels.set(key, channel);
    return channel;
  }

  notify(key: Key): void {
    this._channels.get(key)?.notify();
  }
}

/**
 * Coordinates batched snapshot refreshes and external-store notifications for a download session.
 * It owns publication timing so high-frequency range events do not force every view to update at once.
 */
export class DownloadSessionPublisher {
  private _topologySnapshot: DownloadTopologySnapshot;
  private _summarySnapshot: DownloadSummarySnapshot;
  private readonly _blockSnapshots = new Map<string, DownloadBlockSnapshot>();
  private readonly _trackSnapshots = new Map<string, DownloadTrackSnapshot>();
  private readonly _topologyChannel: ExternalStoreChannel<DownloadTopologySnapshot>;
  private readonly _summaryChannel: ExternalStoreChannel<DownloadSummarySnapshot>;
  private readonly _blockChannel: KeyedExternalStoreChannel<
    string,
    DownloadBlockSnapshot
  >;
  private readonly _trackChannel: KeyedExternalStoreChannel<
    string,
    DownloadTrackSnapshot
  >;
  private readonly _dirtyBlockIds = new Set<string>();
  private readonly _dirtyTrackIds = new Set<string>();
  private _summaryDirty = false;
  private _topologyDirty = false;
  private _flushTimer: ReturnType<typeof setTimeout> | null = null;
  private _lastFlushTime = Number.NEGATIVE_INFINITY;

  readonly topology: ReadonlyExternalStore<DownloadTopologySnapshot>;
  readonly summary: ReadonlyExternalStore<DownloadSummarySnapshot>;

  constructor(
    private readonly _snapshotProvider: SnapshotProvider,
    private readonly _emptyBlockSnapshot: DownloadBlockSnapshot,
    private readonly _emptyTrackSnapshot: DownloadTrackSnapshot,
  ) {
    this._topologySnapshot = _snapshotProvider.createTopologySnapshot();
    this._summarySnapshot = _snapshotProvider.createSummarySnapshot();
    this._topologyChannel = new ExternalStoreChannel(() => this._topologySnapshot);
    this._summaryChannel = new ExternalStoreChannel(() => this._summarySnapshot);
    this._blockChannel = new KeyedExternalStoreChannel(
      (blockId) => this._blockSnapshots.get(blockId) ?? this._emptyBlockSnapshot,
    );
    this._trackChannel = new KeyedExternalStoreChannel(
      (trackId) => this._trackSnapshots.get(trackId) ?? this._emptyTrackSnapshot,
    );
    this.topology = this._topologyChannel;
    this.summary = this._summaryChannel;
  }

  block(blockId: string): ReadonlyExternalStore<DownloadBlockSnapshot> {
    return this._blockChannel.channel(blockId);
  }

  track(trackId: string): ReadonlyExternalStore<DownloadTrackSnapshot> {
    return this._trackChannel.channel(trackId);
  }

  resetSnapshots(): void {
    this._cancelFlush();
    this._blockSnapshots.clear();
    this._trackSnapshots.clear();
    this._dirtyBlockIds.clear();
    this._dirtyTrackIds.clear();
    this._summaryDirty = false;
    this._topologyDirty = false;
  }

  markBlocks(blockIds: Iterable<string>): void {
    for (const blockId of blockIds) {
      this._dirtyBlockIds.add(blockId);
    }
  }

  markTracks(trackIds: Iterable<string>): void {
    for (const trackId of trackIds) {
      this._dirtyTrackIds.add(trackId);
    }
  }

  markSummaryDirty(): void {
    this._summaryDirty = true;
  }

  markTopologyDirty(): void {
    this._topologyDirty = true;
  }

  schedule(): void {
    if (this._flushTimer !== null) {
      return;
    }
    const delay = Math.max(0, updateIntervalMs - (Date.now() - this._lastFlushTime));
    if (delay === 0) {
      this.flush();
      return;
    }
    this._flushTimer = setTimeout(() => {
      this._flushTimer = null;
      this.flush();
    }, delay);
  }

  flush(force = false): void {
    if (
      !force &&
      !this._topologyDirty &&
      !this._summaryDirty &&
      this._dirtyBlockIds.size === 0 &&
      this._dirtyTrackIds.size === 0
    ) {
      return;
    }
    this._cancelFlush();
    this._lastFlushTime = Date.now();

    for (const blockId of this._dirtyBlockIds) {
      this._blockSnapshots.set(
        blockId,
        this._snapshotProvider.createBlockSnapshot(blockId),
      );
    }
    for (const trackId of this._dirtyTrackIds) {
      this._trackSnapshots.set(
        trackId,
        this._snapshotProvider.createTrackSnapshot(trackId),
      );
    }
    if (this._summaryDirty) {
      this._summarySnapshot = this._snapshotProvider.createSummarySnapshot();
    }
    if (this._topologyDirty) {
      this._topologySnapshot = this._snapshotProvider.createTopologySnapshot();
    }

    const dirtyBlockIds = [...this._dirtyBlockIds];
    const dirtyTrackIds = [...this._dirtyTrackIds];
    const notifySummary = this._summaryDirty;
    const notifyTopology = this._topologyDirty;
    this._dirtyBlockIds.clear();
    this._dirtyTrackIds.clear();
    this._summaryDirty = false;
    this._topologyDirty = false;

    if (notifyTopology) {
      this._topologyChannel.notify();
    }
    if (notifySummary) {
      this._summaryChannel.notify();
    }
    for (const blockId of dirtyBlockIds) {
      this._blockChannel.notify(blockId);
    }
    for (const trackId of dirtyTrackIds) {
      this._trackChannel.notify(trackId);
    }
  }

  private _cancelFlush(): void {
    if (this._flushTimer !== null) {
      clearTimeout(this._flushTimer);
      this._flushTimer = null;
    }
  }
}
