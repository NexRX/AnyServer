import type {
  ServerTemplate,
  TemplateListResponse,
  CreateTemplateRequest,
  UpdateTemplateRequest,
  FetchOptionsResponse,
  OptionsSortOrder,
} from "../types/bindings";
import { request } from "./core";
import { getToken } from "./core";

export function listTemplates(): Promise<TemplateListResponse> {
  return request<TemplateListResponse>("GET", "/templates");
}

export function getTemplate(id: string): Promise<ServerTemplate> {
  return request<ServerTemplate>("GET", `/templates/${encodeURIComponent(id)}`);
}

export function createTemplate(
  req: CreateTemplateRequest,
): Promise<ServerTemplate> {
  return request<ServerTemplate>("POST", "/templates", req);
}

export function updateTemplate(
  id: string,
  req: UpdateTemplateRequest,
): Promise<ServerTemplate> {
  return request<ServerTemplate>(
    "PUT",
    `/templates/${encodeURIComponent(id)}`,
    req,
  );
}

export function deleteTemplate(
  id: string,
): Promise<{ deleted: boolean; id: string }> {
  return request<{ deleted: boolean; id: string }>(
    "DELETE",
    `/templates/${encodeURIComponent(id)}`,
  );
}

export async function fetchOptions(opts: {
  url: string;
  path?: string | null;
  value_key?: string | null;
  label_key?: string | null;
  sort?: OptionsSortOrder | null;
  limit?: number | null;
  params?: Record<string, string>;
}): Promise<FetchOptionsResponse> {
  const qp = new URLSearchParams();
  qp.set("url", opts.url);
  if (opts.path) qp.set("path", opts.path);
  if (opts.value_key) qp.set("value_key", opts.value_key);
  if (opts.label_key) qp.set("label_key", opts.label_key);
  if (opts.sort) qp.set("sort", opts.sort);
  if (opts.limit != null) qp.set("limit", String(opts.limit));
  if (opts.params && Object.keys(opts.params).length > 0) {
    qp.set("params", JSON.stringify(opts.params));
  }

  const headers: Record<string, string> = {};
  const token = getToken();
  if (token) {
    headers["Authorization"] = `Bearer ${token}`;
  }

  const res = await fetch(`/api/templates/fetch-options?${qp.toString()}`, {
    headers,
  });

  if (!res.ok) {
    const err = await res.json().catch(() => ({
      error: res.statusText || `HTTP ${res.status}`,
    }));
    throw new Error(err.error ?? `HTTP ${res.status}`);
  }

  return res.json() as Promise<FetchOptionsResponse>;
}
