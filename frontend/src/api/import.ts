import type {
  ImportUrlRequest,
  ImportUrlResponse,
  ImportFolderRequest,
  ImportFolderResponse,
} from "../types/bindings";
import { request } from "./core";

export function importFromUrl(
  req: ImportUrlRequest,
): Promise<ImportUrlResponse> {
  return request<ImportUrlResponse>("POST", "/import/url", req);
}

export function importFromFolder(
  req: ImportFolderRequest,
): Promise<ImportFolderResponse> {
  return request<ImportFolderResponse>("POST", "/import/folder", req);
}
