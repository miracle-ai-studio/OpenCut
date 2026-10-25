import { sourceThumbnail } from "./source-thumbnail";
import { TaskFeed } from "./task-feed";
import { Api } from "./scene/api";
import { ProjectCovers } from "./project-covers";
import type { CoverRequest } from "./project-covers";
import { CoverSceneViews } from "../shared/view-preferences";
import { OpenCutClient, createEditorClient } from "../../lib/editor-sdk";
import type { EditorConnection, ToolOutput } from "../../lib/editor-sdk";
import { normalizeHistorical } from "../model/types";
import type { ProjectRef, Snapshot } from "../model/types";

/** Recovery scope after `authenticateConnection()` resolved; a missing identity is a caller error. */
export function authenticatedIdentity(api: { readonly connectionIdentity: string | null }): string {
  const identity = api.connectionIdentity;
  if (identity === null) throw new Error("The connection is not authenticated.");
  return identity;
}

export class EditorApi {
  private authenticatedIdentity: string | null = null;
  /** In-memory recovery scope, established only by the authenticated service.
   * It contains service/principal/credential identity, never the bearer token. */
  get connectionIdentity() { return this.authenticatedIdentity; }
  async authenticateConnection() {
    if (this.authenticatedIdentity !== null) return;
    const access = await this.sdk.call("opencut_access_inspect", {});
    if (this.authenticatedIdentity !== null) return;
    if (!access.principal_id || typeof access.owner !== "boolean" ||
        (access.credential_id != null && typeof access.credential_id !== "string"))
      throw new Error("The service returned an invalid connection identity.");
    this.authenticatedIdentity = JSON.stringify([
      this.sdk.baseUrl, access.principal_id, access.credential_id ?? null, access.owner,
    ]);
  }
  readonly covers = new ProjectCovers((project, signal) =>
    this.projectCover(project, signal),
  );
  readonly sdk: OpenCutClient;
  readonly audioPreviewConnection: EditorConnection;
  readonly scene: Api;
  readonly coverViews: CoverSceneViews;
  private feed: TaskFeed;
  constructor(endpoint: string, token: string) {
    this.audioPreviewConnection = { baseUrl: endpoint, token };
    this.sdk = createEditorClient(this.audioPreviewConnection);
    this.coverViews = new CoverSceneViews(this.sdk.baseUrl);
    this.scene = new Api(this.sdk);
    this.feed = new TaskFeed(this.sdk);
  }
  async projects(signal?: AbortSignal): Promise<ProjectRef[]> {
    const items: ProjectRef[] = [];
    let cursor: string | null = null;
    do {
      const page: ToolOutput<"opencut_project_list"> = await this.sdk.call(
        "opencut_project_list",
        { cursor, limit: 100 },
        { signal },
      );
      items.push(
        ...page.items.map((item) => ({
          id: item.project_id,
          name: item.name,
          revision: item.revision,
          format: item.format,
          library: item,
        })),
      );
      cursor = page.next_cursor ?? null;
    } while (cursor);
    this.covers.retain(items);
    return items;
  }

  private async projectCover(project:CoverRequest,signal:AbortSignal):Promise<Blob|null> {
    if(project.format !== "editor") return null;
    const snapshot=await this.sdk.inspectEditorProject({project_id:project.id,revision:project.revision},{signal});
    if(snapshot.revision!==project.revision || snapshot.project.id!==project.id) throw Error("Project cover revision changed");
    const sequenceId=snapshot.project.sequences.some(scene=>scene.id===project.coverSequenceId)
      ? project.coverSequenceId! : snapshot.project.default_sequence_id;
    const scene=snapshot.project.sequences.find(scene=>scene.id===sequenceId);
    if(!scene) throw Error("Project cover scene unavailable");
    const assetIds = scene.tracks.filter(track => track.enabled).flatMap(track => track.elements.filter(element => element.enabled).flatMap(element => "asset_id" in element.content ? [element.content.asset_id] : []));
    const asset = assetIds.map(id => snapshot.project.assets.find(item => item.id === id)).find(item => item && (item.kind === "image" || item.kind === "video"));
    if (!asset || (asset.kind !== "image" && asset.kind !== "video")) return null;
    const resource = await this.scene.resource(project.id, project.revision, asset.id, signal);
    signal.throwIfAborted();
    if (resource.project_id !== project.id || resource.revision !== project.revision || resource.resource_id !== asset.id || resource.version.id !== asset.version.id || resource.version.content_hash !== asset.version.content_hash || resource.version.size !== asset.version.size) throw Error("Project cover resource changed");
    const url = this.sdk.projectResourceUrl(resource);
    if (new URL(url).origin !== location.origin) return null;
    return sourceThumbnail(url, asset.kind, signal);
  }

  async inspect(project:ProjectRef,revision?:string,signal?:AbortSignal):Promise<Snapshot>{
    const view=await this.sdk.call("opencut_project_inspect",{
      project_id:project.id,revision:revision??null,
    },{signal});
    if(view.kind!=="historical")throw new Error("Project content changed;open it from the project library.");
    return normalizeHistorical(view.snapshot);
  }
  create(name: string, width: number, height: number, fps: number) {
    return this.sdk.call("opencut_project_create", {
      project: {id: crypto.randomUUID(),name,sequence_id: crypto.randomUUID(),width,height,
        fps: {numerator: fps,denominator: 1}},
      idempotency_key: crypto.randomUUID(),
    });
  }
  async history(projectId: string, fromRevision: string | null = null) {
    const page = await this.sdk.call("opencut_project_history", {
      project_id: projectId,
      from_revision: fromRevision,
      limit: 50,
    });
    const navigation = fromRevision
      ? null
      : await this.sdk.call("opencut_project_history_navigation", {
          project_id: projectId,
          revision: page.head_revision,
        });
    return { ...page, navigation };
  }
  tasks(projectId: string, signal?: AbortSignal) {
    return this.feed.read(projectId, signal);
  }
}
