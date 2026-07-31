import { describe, expect, it } from "vitest";

import {
  reduceDatasetSessionState,
  type DatasetSessionState,
} from "./datasetSessionState";

interface TestSession {
  id: string;
}

const committedSession: TestSession = { id: "committed" };

function createState(): DatasetSessionState<TestSession> {
  return {
    committed: committedSession,
    loadError: null,
    loading: false,
    requestVersion: 0,
  };
}

describe("reduceDatasetSessionState", () => {
  it("keeps the committed session while a request starts", () => {
    const state = reduceDatasetSessionState(createState(), {
      type: "request-started",
      requestVersion: 1,
    });

    expect(state.committed).toBe(committedSession);
    expect(state.loading).toBe(true);
  });

  it("keeps the committed session when the latest request fails", () => {
    const loadingState = reduceDatasetSessionState(createState(), {
      type: "request-started",
      requestVersion: 1,
    });
    const failedState = reduceDatasetSessionState(loadingState, {
      type: "request-failed",
      requestVersion: 1,
      error: new Error("failed"),
    });

    expect(failedState.committed).toBe(committedSession);
    expect(failedState.loading).toBe(false);
    expect(failedState.loadError?.message).toBe("failed");
  });

  it("ignores stale success and failure events", () => {
    const loadingState = reduceDatasetSessionState(createState(), {
      type: "request-started",
      requestVersion: 2,
    });
    const staleSuccess = reduceDatasetSessionState(loadingState, {
      type: "request-succeeded",
      requestVersion: 1,
      session: { id: "stale" },
    });
    const staleFailure = reduceDatasetSessionState(staleSuccess, {
      type: "request-failed",
      requestVersion: 1,
      error: new Error("stale"),
    });

    expect(staleFailure).toBe(loadingState);
  });

  it("commits the latest successful session", () => {
    const nextSession = { id: "next" };
    const loadingState = reduceDatasetSessionState(createState(), {
      type: "request-started",
      requestVersion: 1,
    });
    const succeededState = reduceDatasetSessionState(loadingState, {
      type: "request-succeeded",
      requestVersion: 1,
      session: nextSession,
    });

    expect(succeededState.committed).toBe(nextSession);
    expect(succeededState.loading).toBe(false);
    expect(succeededState.loadError).toBeNull();
  });
});
