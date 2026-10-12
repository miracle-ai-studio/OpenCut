import type { MigrationResult, ProjectRecord } from "./types";
import { isRecord } from "./utils";

interface V1Scene {
	id: string;
	name: string;
	isMain: boolean;
	tracks: unknown[];
	bookmarks: unknown[];
	createdAt: string;
	updatedAt: string;
}

export interface TransformV0ToV1Options {
	now?: Date;
 sceneId: string;
}

export function transformProjectV0ToV1({
	project,
	options,
}: {
	project: ProjectRecord;
	options: TransformV0ToV1Options;
}): MigrationResult<ProjectRecord> {
	const { now = new Date(0) } = options;

	const scenesValue = project.scenes;
	if (Array.isArray(scenesValue) && scenesValue.length > 0) {
		return { project, skipped: true, reason: "already has scenes" };
	}

	const sceneId = options.sceneId;
	const sceneCreatedAt = now.toISOString();
	const sceneUpdatedAt = now.toISOString();

	const mainScene: V1Scene = {
		id: sceneId,
		name: "Main scene",
		isMain: true,
		tracks: [],
		bookmarks: [],
		createdAt: sceneCreatedAt,
		updatedAt: sceneUpdatedAt,
	};

	const updatedProject: ProjectRecord = {
		...project,
		scenes: [mainScene],
		currentSceneId: sceneId,
		version: 1,
	};

	const updatedAt = now.toISOString();
	if (isRecord(project.metadata)) {
		updatedProject.metadata = {
			...project.metadata,
			updatedAt,
		};
	} else {
		updatedProject.updatedAt = updatedAt;
	}

	return { project: updatedProject, skipped: false };
}

export { getProjectId } from "./utils";
