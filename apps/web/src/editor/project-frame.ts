export type ProjectFrameRequestIdentity = {
  projectId: string;
  revision: string;
  generation: number;
};

export const isCurrentProjectFrameRequest = (
  request: ProjectFrameRequestIdentity,
  current: ProjectFrameRequestIdentity,
) =>
  request.projectId === current.projectId &&
  request.revision === current.revision &&
  request.generation === current.generation;
