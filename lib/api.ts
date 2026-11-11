import { nativeModule } from "nsh";
import type {
  CacheClearReport,
  ConnectivityCheck,
  DisplayWorkArea,
  Installation,
  InstallationDetail,
  LeaderboardKind,
  MarketplaceSkill,
  OperationEvent,
  OperationSnapshot,
  Settings,
  SettingsPatch,
  SkillPreview,
  SkillSource,
  UpdateConflictPolicy,
} from "./types";

interface NativeModules {
  AppNative: {
    schemaVersion(): number;
    primaryDisplayWorkArea(): unknown;
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
    listInstallations(): Promise<unknown>;
    installationDetail(id: string): Promise<unknown>;
    renameInstallation(id: string, newName: string): unknown;
    deleteInstallations(ids: unknown, confirmModified: boolean): unknown;
    checkUpdates(ids: unknown): unknown;
    applyUpdate(id: string, conflictPolicy: string): unknown;
  };
}

const { AppNative } = nativeModule<NativeModules>("skills-manager-nsh/native");

export function schemaVersion(): number {
  return AppNative.schemaVersion();
}

export function primaryDisplayWorkArea(): DisplayWorkArea | null {
  return AppNative.primaryDisplayWorkArea() as DisplayWorkArea | null;
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

export async function listInstallations(): Promise<Installation[]> {
  return await AppNative.listInstallations() as Installation[];
}

export async function installationDetail(id: string): Promise<InstallationDetail> {
  return await AppNative.installationDetail(id) as InstallationDetail;
}

export function renameInstallation(id: string, newName: string): string {
  return AppNative.renameInstallation(id, newName) as string;
}

export function deleteInstallations(ids: string[], confirmModified = false): string {
  return AppNative.deleteInstallations(ids, confirmModified) as string;
}

export function checkUpdates(ids: string[]): string {
  return AppNative.checkUpdates(ids) as string;
}

export function applyUpdate(id: string, conflictPolicy: UpdateConflictPolicy): string {
  return AppNative.applyUpdate(id, conflictPolicy) as string;
}