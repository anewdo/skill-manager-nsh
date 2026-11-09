import { nativeModule } from "nsh";
import type {
  Category,
  Installation,
  InstallationDetail,
  LeaderboardKind,
  MarketplaceSkill,
  OperationEvent,
  OperationSnapshot,
  Settings,
  SkillPreview,
  SkillSource,
  TrashEntry,
} from "./types";

interface NativeModules {
  AppNative: {
    schemaVersion(): number;
    marketLeaderboard(kind: string, forceRefresh: boolean): Promise<unknown>;
    marketSearch(query: string, limit: number, forceRefresh: boolean): Promise<unknown>;
    marketPreview(source: unknown): Promise<unknown>;
    installMarketplace(source: unknown): unknown;
    activeOperation(): unknown;
    operationSnapshot(id: string): unknown;
    cancelOperation(id: string): unknown;
    subscribeOperations(onEvent: (value: OperationEvent) => void): Promise<void>;
    getSettings(): Promise<unknown>;
    listInstallations(category: string): Promise<unknown>;
    installationDetail(id: string): Promise<unknown>;
    renameInstallation(id: string, newName: string): unknown;
    deleteInstallations(ids: unknown, confirmModified: boolean): unknown;
    listTrash(): Promise<unknown>;
    restoreTrash(ids: unknown): unknown;
    purgeTrash(ids: unknown): unknown;
  };
}

const { AppNative } = nativeModule<NativeModules>("skills-manager-nsh/native");

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

export function installMarketplace(source: SkillSource): string {
  return AppNative.installMarketplace(source) as string;
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

export async function listInstallations(category: Category): Promise<Installation[]> {
  return await AppNative.listInstallations(category) as Installation[];
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

export async function listTrash(): Promise<TrashEntry[]> {
  return await AppNative.listTrash() as TrashEntry[];
}

export function restoreTrash(ids: string[]): string {
  return AppNative.restoreTrash(ids) as string;
}

export function purgeTrash(ids: string[]): string {
  return AppNative.purgeTrash(ids) as string;
}
