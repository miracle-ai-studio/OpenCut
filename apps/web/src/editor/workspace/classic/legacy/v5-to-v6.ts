import type { MigrationResult, ProjectRecord } from "./types";
import { getProjectId, isRecord } from "./utils";

export function transformProjectTo({
	project,
}: {
	project: ProjectRecord;
}): MigrationResult<ProjectRecord> {
	const projectId = getProjectId({ project });
	if (!projectId) {
		return { project, skipped: true, reason: "no project id" };
	}

	if (isProject({ project })) {
		return { project, skipped: true, reason: "already v6" };
	}

	const migratedProject = migrateProjectBookmarks({ project });

	return {
		project: {
			...migratedProject,
			version: 6,
		},
		skipped: false,
	};
}

function migrateProjectBookmarks({
	project,
}: {
	project: ProjectRecord;
}): ProjectRecord {
	const scenesValue = project.scenes;
	if (!Array.isArray(scenesValue)) {
		return project;
	}

	let hasSceneChanges = false;
	const migratedScenes = scenesValue.map((scene) => {
		const migratedScene = migrateSceneBookmarks({ scene });
		if (migratedScene !== scene) {
			hasSceneChanges = true;
		}
		return migratedScene;
	});

	if (!hasSceneChanges) {
		return project;
	}

	return {
		...project,
		scenes: migratedScenes,
	};
}

function migrateSceneBookmarks({ scene }: { scene: unknown }): unknown {
	if (!isRecord(scene)) {
		return scene;
	}

	const bookmarksValue = scene.bookmarks;
	if (!Array.isArray(bookmarksValue)) {
		return scene;
	}

	const needsMigration = bookmarksValue.some(
		(bookmark) => typeof bookmark === "number",
	);
	if (!needsMigration) {
		return scene;
	}

	const migratedBookmarks = bookmarksValue.map((bookmark) =>
		typeof bookmark === "number" ? { time: bookmark } : bookmark,
	);

	return {
		...scene,
		bookmarks: migratedBookmarks,
	};
}

function isProject({ project }: { project: ProjectRecord }): boolean {
	const versionValue = project.version;
	return typeof versionValue === "number" && versionValue >= 6;
}
