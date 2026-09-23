export type WorkspaceIdentity<Connection> = {
  connection: Connection;
  projectId: string;
  revision: string;
  generation: number;
};

export const isCurrentWorkspace = <Connection>(
  request: WorkspaceIdentity<Connection>,
  current: WorkspaceIdentity<Connection> | null,
) =>
  current !== null &&
  request.connection === current.connection &&
  request.projectId === current.projectId &&
  request.revision === current.revision &&
  request.generation === current.generation;

export const isSameWorkspaceVisit = <Connection>(
  request: WorkspaceIdentity<Connection>,
  current: WorkspaceIdentity<Connection> | null,
) =>
  current !== null &&
  request.connection === current.connection &&
  request.projectId === current.projectId &&
  request.generation === current.generation;

export const loadCurrentSourcePreview = async <Connection>(options: {
  request: WorkspaceIdentity<Connection>;
  current: () => WorkspaceIdentity<Connection> | null;
  load: () => Promise<string>;
  revoke: (url: string) => void;
}) => {
  let url: string;
  try {
    url = await options.load();
  } catch (error) {
    return {
      url: null,
      error: error instanceof Error ? error : new Error(String(error)),
    };
  }
  if (!isCurrentWorkspace(options.request, options.current())) {
    options.revoke(url);
    return { url: null, error: null };
  }
  return { url, error: null };
};

export const prepareCurrentPreviewRequest = async <Connection>(options: {
  request: WorkspaceIdentity<Connection>;
  current: () => WorkspaceIdentity<Connection> | null;
  prepare: () => Promise<boolean>;
}) => {
  const ready = await options.prepare();
  return ready && isCurrentWorkspace(options.request, options.current());
};

export const readCurrentProjectState = async <
  Connection,
  Snapshot,
  History,
>(options: {
  request: WorkspaceIdentity<Connection>;
  current: () => WorkspaceIdentity<Connection> | null;
  readSnapshot: () => Promise<Snapshot>;
  readHistory: () => Promise<History>;
}) => {
  const [snapshot, history] = await Promise.all([
    options.readSnapshot(),
    options.readHistory(),
  ]);
  return isCurrentWorkspace(options.request, options.current())
    ? { snapshot, history }
    : null;
};
