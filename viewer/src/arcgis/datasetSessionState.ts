export interface DatasetSessionState<Session> {
  committed: Session;
  loadError: Error | null;
  loading: boolean;
  requestVersion: number;
}

export type DatasetSessionEvent<Session> =
  | { type: "request-started"; requestVersion: number }
  | {
      type: "request-succeeded";
      requestVersion: number;
      session: Session;
    }
  | {
      type: "request-failed";
      requestVersion: number;
      error: Error;
    }
  | { type: "session-refreshed"; session: Session };

export function reduceDatasetSessionState<Session>(
  state: DatasetSessionState<Session>,
  event: DatasetSessionEvent<Session>,
): DatasetSessionState<Session> {
  switch (event.type) {
    case "request-started":
      return {
        ...state,
        loadError: null,
        loading: true,
        requestVersion: event.requestVersion,
      };
    case "request-succeeded":
      return event.requestVersion === state.requestVersion
        ? {
            committed: event.session,
            loadError: null,
            loading: false,
            requestVersion: state.requestVersion,
          }
        : state;
    case "request-failed":
      return event.requestVersion === state.requestVersion
        ? {
            ...state,
            loadError: event.error,
            loading: false,
          }
        : state;
    case "session-refreshed":
      return event.session === state.committed ? { ...state } : state;
  }
}
