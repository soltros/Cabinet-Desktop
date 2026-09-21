export interface User {
  id: string;
  username: string;
  role: string;
  quota: number;
  usedSpace: number;
}
export interface Session { serverUrl: string; user: User; }
export interface CabinetFile {
  id: string; ownerId: string; name: string; extension?: string | null;
  mimeType?: string | null; size: number; hash?: string | null;
  parentId?: string | null; thumbnail?: string | null;
  createdAt: string; updatedAt: string;
}
export interface Folder {
  id: string; ownerId: string; name: string; parentId?: string | null; createdAt: string;
}
export interface AdminStats {
  totalUsers: number; totalFiles: number; totalShares: number;
  totalStorageUsed: number; totalStorageQuota: number;
}
export interface AdminUser {
  id: string; username: string; role: string; quota: number; usedSpace: number;
}
export interface AdminShare {
  id: string; fileId: string; creatorId: string; downloads: number;
  fileName?: string; creatorName?: string; fileSize?: number;
}
