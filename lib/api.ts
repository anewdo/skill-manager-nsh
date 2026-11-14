import { nativeModule } from "nsh";
import type {
  CacheClearReport,
  LocalDiscovery,
  ConnectivityCheck,
  InstalledSkill,
  SkillDetail,
  LeaderboardKind,
  MarketplaceSkill,
  OperationEvent,
  OperationSnapshot,
  Settings,
  SettingsPatch,
  SkillPreview,
  SkillSource,
  UpdateConflictPolicy,
  UrlDiscovery,
} from "./types";

interface NativeModules {
  AppNative: {
    schemaVersion(): number;
    discoverUrlImport(input: string): Promise<unknown>;
    discardUrlImport(): Promise<void>;
    urlImportSnapshot(): Promise<unknown>;
    importUrlCandidates(sessionId: string, ids: unknown): unknown;
    chooseLocalImport(kind: string): Promise<unknown>;
    discoverLocalImport(paths: unknown): Promise<unknown>;
    rescanLocalImport(): Promise<unknown>;
    localImportSnapshot(): Promise<unknown>;
    discardLocalImport(): Promise<unknown>;
    importLocalCandidates(ids: unknown): unknown;
    shutdownLocalImport(): Promise<void>;
    marketLeaderboard(kind: string, forceRefresh: boolean): Promise<unknown>;
    marketSearch(query: string, limit: number, forceRefresh: boolean): Promise<unknown>;
    marketPreview(source: unknown): Promise<unknown>;
    installFromSource(source: unknown, label: unknown): unknown;
    activeOperation(): unknown;
    operationSnapshot(id: string): unknown;
    cancelOperation(id: string): unknown;
    subscribeOperations(onEvent: (value: OperationEvent) => void): Promise<void>;
    getSettings(): Promise<unknown>;
    updateSettings(patch: unknown): Promise<unknown>;
    clearCache(): Promise<unknown>;
    testConnectivity(): Promise<unknown>;
    dataDirectory(): Promise<unknown>;
    listSkills(): Promise<unknown>;
    skillDetail(id: string): Promise<unknown>;
    renameSkill(id: string, newName: string): unknown;
    deleteSkills(ids: unknown, confirmModified: boolean): unknown;
    checkUpdates(ids: unknown): unknown;
    applyUpdate(id: string, conflictPolicy: string): unknown;
  };
}

const { AppNative } = nativeModule<NativeModules>("skills-manager-nsh/native");

export async function discoverUrlImport(input: string): Promise<UrlDiscovery> {
  return await AppNative.discoverUrlImport(input) as UrlDiscovery;
}

export async function discardUrlImport(): Promise<void> {
  await AppNative.discardUrlImport();
}

export async function urlImportSnapshot(): Promise<UrlDiscovery | null> {
  return await AppNative.urlImportSnapshot() as UrlDiscovery | null;
}

export function importUrlCandidates(sessionId: string, ids: string[]): string {
  return AppNative.importUrlCandidates(sessionId, ids) as string;
}

export async function chooseLocalImport(kind: "folder" | "zip"): Promise<LocalDiscovery | null> {
  return await AppNative.chooseLocalImport(kind) as LocalDiscovery | null;
}

export async function discoverLocalImport(paths: string[]): Promise<LocalDiscovery> {
  return await AppNative.discoverLocalImport(paths) as LocalDiscovery;
}

export async function rescanLocalImport(): Promise<LocalDiscovery> {
  return await AppNative.rescanLocalImport() as LocalDiscovery;
}

export async function localImportSnapshot(): Promise<LocalDiscovery> {
  return await AppNative.localImportSnapshot() as LocalDiscovery;
}

export async function discardLocalImport(): Promise<LocalDiscovery> {
  return await AppNative.discardLocalImport() as LocalDiscovery;
}

export function importLocalCandidates(ids: string[]): string {
  return AppNative.importLocalCandidates(ids) as string;
}

export async function shutdownLocalImport(): Promise<void> {
  return await AppNative.shutdownLocalImport();
}

export function schemaVersion(): number {
  return AppNative.schemaVersion();
}

export async function marketLeaderboard(
  kind: LeaderboardKind,
  forceRefresh = false,
): Promise<MarketplaceSkill[]> {
  return await AppNative.marketLeaderboard(kind, forceRefresh) as MarketplaceSkill[];
}

export async function marketSearch(
  query: string,
  limit = 60,
  forceRefresh = false,
): Promise<MarketplaceSkill[]> {
  return await AppNative.marketSearch(query, limit, forceRefresh) as MarketplaceSkill[];
}

export async function marketPreview(source: SkillSource): Promise<SkillPreview> {
  return await AppNative.marketPreview(source) as SkillPreview;
}

export function installFromSource(
  source: SkillSource,
  label: { name: string; source: string },
): string {
  return AppNative.installFromSource(source, label) as string;
}

export function activeOperation(): OperationSnapshot | null {
  return AppNative.activeOperation() as OperationSnapshot | null;
}

export function operationSnapshot(id: string): OperationSnapshot | null {
  return AppNative.operationSnapshot(id) as OperationSnapshot | null;
}

export function cancelOperation(id: string): OperationSnapshot {
  return AppNative.cancelOperation(id) as OperationSnapshot;
}

export async function subscribeOperations(
  onEvent: (event: OperationEvent) => void,
): Promise<void> {
  return await AppNative.subscribeOperations(onEvent);
}

export async function getSettings(): Promise<Settings> {
  return await AppNative.getSettings() as Settings;
}

export async function updateSettings(patch: SettingsPatch): Promise<Settings> {
  return await AppNative.updateSettings(patch) as Settings;
}

export async function clearCache(): Promise<CacheClearReport> {
  return await AppNative.clearCache() as CacheClearReport;
}

export async function testConnectivity(): Promise<ConnectivityCheck[]> {
  return await AppNative.testConnectivity() as ConnectivityCheck[];
}

export async function dataDirectory(): Promise<string> {
  return await AppNative.dataDirectory() as string;
}

export async function listSkills(): Promise<InstalledSkill[]> {
  return await AppNative.listSkills() as InstalledSkill[];
}

export async function skillDetail(id: string): Promise<SkillDetail> {
  return await AppNative.skillDetail(id) as SkillDetail;
}

export function renameSkill(id: string, newName: string): string {
  return AppNative.renameSkill(id, newName) as string;
}

export function deleteSkills(ids: string[], confirmModified = false): string {
  return AppNative.deleteSkills(ids, confirmModified) as string;
}

export function checkUpdates(ids: string[]): string {
  return AppNative.checkUpdates(ids) as string;
}

export function applyUpdate(id: string, conflictPolicy: UpdateConflictPolicy): string {
  return AppNative.applyUpdate(id, conflictPolicy) as string;
}
