import { invoke } from "@tauri-apps/api/core";
import type { AdminShare, AdminStats, AdminUser, CabinetFile, Folder, Session } from "./types";

export const api = {
  restoreSession: () => invoke<Session | null>("restore_session"),
  configureServer: (serverUrl: string) => invoke<string>("configure_server", { serverUrl }),
  login: (serverUrl: string, username: string, password: string) =>
    invoke<Session>("login", { serverUrl, username, password }),
  logout: () => invoke<void>("logout"),
  listFiles: () => invoke<CabinetFile[]>("list_files"),
  listFolders: () => invoke<Folder[]>("list_folders"),
  createFolder: (name: string, parentId: string | null) =>
    invoke<Folder>("create_folder", { name, parentId }),
  uploadFile: (path: string, parentId: string | null) =>
    invoke<CabinetFile>("upload_file", { path, parentId }),
  downloadFile: (id: string, destination: string) =>
    invoke<void>("download_file", { id, destination }),
  renameFile: (id: string, name: string) =>
    invoke<CabinetFile>("rename_file", { id, name }),
  deleteFile: (id: string) => invoke<void>("delete_file", { id }),
  internalShare: (id: string, username: string) =>
    invoke<void>("internal_share", { id, username }),
  createPublicShare: (fileId: string) =>
    invoke<{ link: string }>("create_public_share", { fileId }),
  thumbnailDataUrl: (id: string) =>
    invoke<string | null>("thumbnail_data_url", { id }),
  adminStats: () => invoke<AdminStats>("admin_stats"),
  adminUsers: () => invoke<AdminUser[]>("admin_users"),
  adminShares: () => invoke<AdminShare[]>("admin_shares"),
  adminLogs: () => invoke<string>("admin_logs")
};
