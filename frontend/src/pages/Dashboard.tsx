import {
  type Component,
  createResource,
  createSignal,
  For,
  Show,
  onCleanup,
  onMount,
  createEffect,
} from "solid-js";
import { A, useSearchParams } from "@solidjs/router";
import { listServers, getUpdateStatus } from "../api/client";
import type {
  ServerWithStatus,
  ServerRuntime,
  UpdateCheckResult,
} from "../types/bindings";
import ServerCard from "../components/ServerCard";
import Loader from "../components/Loader";
import ConnectionBanner from "../components/ConnectionBanner";
import { useGlobalEvents } from "../hooks/useGlobalEvents";

const AUTO_REFRESH_KEY = "anyserver:dashboard:auto-refresh";
const AUTO_REFRESH_INTERVAL_MS = 60_000;

/** Extract a single string from a search-param value (which may be string | string[] | undefined). */
const param = (v: string | string[] | undefined): string | undefined =>
  Array.isArray(v) ? v[0] : v;

const Dashboard: Component = () => {
  const [searchParams, setSearchParams] = useSearchParams();

  // Pagination state from URL
  const page = () => parseInt(param(searchParams.page) || "1", 10);
  const perPage = () => parseInt(param(searchParams.per_page) || "25", 10);
  const search = () => param(searchParams.search) || "";
  const status = () => param(searchParams.status) || "";
  const sort = () => param(searchParams.sort) || "name";
  const order = () => param(searchParams.order) || "asc";

  const [searchInput, setSearchInput] = createSignal(search());
  let searchDebounceTimer: ReturnType<typeof setTimeout> | null = null;

  const fetchServers = () =>
    listServers({
      page: page(),
      per_page: perPage(),
      search: search() || undefined,
      status: status() || undefined,
      sort: sort(),
      order: order(),
    });

  const [data, { refetch }] = createResource(fetchServers);

  createEffect(() => {
    const input = searchInput();
    if (searchDebounceTimer) {
      clearTimeout(searchDebounceTimer);
    }
    searchDebounceTimer = setTimeout(() => {
      if (input !== search()) {
        setSearchParams({ search: input || undefined, page: "1" });
      }
    }, 300);
  });

  onCleanup(() => {
    if (searchDebounceTimer) {
      clearTimeout(searchDebounceTimer);
    }
  });

  const [updateResults, setUpdateResults] = createSignal<
    Record<string, UpdateCheckResult>
  >({});

  const fetchUpdateStatus = async () => {
    try {
      const res = await getUpdateStatus();
      const map: Record<string, UpdateCheckResult> = {};
      for (const r of res.results) {
        map[r.server_id] = r;
      }
      setUpdateResults(map);
    } catch {}
  };

  onMount(() => {
    fetchUpdateStatus();
  });

  // ── Auto-refresh toggle (persisted in localStorage) ──
  const [autoRefresh, setAutoRefresh] = createSignal(
    localStorage.getItem(AUTO_REFRESH_KEY) === "true",
  );

  let autoRefreshInterval: ReturnType<typeof setInterval> | null = null;

  const clearAutoRefreshInterval = () => {
    if (autoRefreshInterval) {
      clearInterval(autoRefreshInterval);
      autoRefreshInterval = null;
    }
  };

  createEffect(() => {
    clearAutoRefreshInterval();
    if (autoRefresh()) {
      autoRefreshInterval = setInterval(
        () => refetch(),
        AUTO_REFRESH_INTERVAL_MS,
      );
    }
  });

  onCleanup(clearAutoRefreshInterval);

  const toggleAutoRefresh = () => {
    const next = !autoRefresh();
    setAutoRefresh(next);
    localStorage.setItem(AUTO_REFRESH_KEY, String(next));
  };

  // ── Manual refresh ──
  const [refreshing, setRefreshing] = createSignal(false);

  const handleRefresh = async () => {
    if (refreshing()) return;
    setRefreshing(true);
    try {
      await refetch();
    } finally {
      setRefreshing(false);
    }
  };

  // ── Global events WebSocket ──
  // Owns the WebSocket lifecycle. Provides real-time status updates for all
  // servers and handles reconnection automatically. Replaces ~100 lines of
  // hand-rolled WebSocket management that was previously in this file.
  const globalEvents = useGlobalEvents({
    onReconnect: () => refetch(),
  });

  const patchServer = (server: ServerWithStatus): ServerWithStatus => {
    const override_ = globalEvents.runtimeOverrides()[server.server.id];
    if (override_) {
      return { ...server, runtime: override_ };
    }
    return server;
  };

  const handlePageChange = (newPage: number) => {
    setSearchParams({ page: newPage.toString() });
  };

  const handlePerPageChange = (newPerPage: number) => {
    setSearchParams({ per_page: newPerPage.toString(), page: "1" });
  };

  const handleStatusChange = (newStatus: string) => {
    setSearchParams({ status: newStatus || undefined, page: "1" });
  };

  const handleSortChange = (newSort: string) => {
    const newOrder = newSort === sort() && order() === "asc" ? "desc" : "asc";
    setSearchParams({ sort: newSort, order: newOrder, page: "1" });
  };

  const showFilters = () => {
    const resolved = data();
    return resolved && Number(resolved.total) > 10;
  };

  return (
    <div class="dashboard">
      <ConnectionBanner
        state={globalEvents.connectionState()}
        reconnectInfo={globalEvents.reconnectInfo()}
      />

      <div class="page-header">
        <h1>Servers</h1>
        <div class="page-header-actions">
          <div class="refresh-btn-group">
            <button
              class="btn btn-sm refresh-btn-left"
              onClick={handleRefresh}
              disabled={refreshing()}
              title="Refresh server list"
            >
              <span
                classList={{ "refresh-icon-spin": refreshing() }}
                style={{ display: "inline-block" }}
              >
                ↻
              </span>{" "}
              Refresh
            </button>
            <button
              class={`btn btn-sm refresh-btn-right ${autoRefresh() ? "btn-auto-refresh-on" : ""}`}
              onClick={toggleAutoRefresh}
              title={
                autoRefresh()
                  ? "Auto-refresh is on (every 60s) — click to disable"
                  : "Enable auto-refresh (every 60s)"
              }
            >
              <span
                class={
                  autoRefresh() ? "auto-refresh-dot active" : "auto-refresh-dot"
                }
              />
              Auto
            </button>
          </div>
          <A href="/create" class="btn btn-primary btn-sm">
            + New Server
          </A>
        </div>
      </div>

      <div
        class="dashboard-layout"
        classList={{ "with-sidebar": showFilters() }}
      >
        <Show when={showFilters()}>
          <aside class="dashboard-sidebar">
            <div class="sidebar-section">
              <h3 class="sidebar-title">
                <span class="sidebar-icon">🔍</span>
                Filters
              </h3>
              <div class="sidebar-filters">
                <div class="sidebar-filter-group">
                  <label class="sidebar-label">Search</label>
                  <input
                    type="text"
                    class="search-input"
                    placeholder="Search servers..."
                    value={searchInput()}
                    onInput={(e) => setSearchInput(e.currentTarget.value)}
                  />
                </div>
                <div class="sidebar-filter-group">
                  <label class="sidebar-label">Status</label>
                  <select
                    class="status-filter"
                    value={status()}
                    onChange={(e) => handleStatusChange(e.currentTarget.value)}
                  >
                    <option value="">All Statuses</option>
                    <option value="running">Running</option>
                    <option value="stopped">Stopped</option>
                    <option value="starting">Starting</option>
                    <option value="stopping">Stopping</option>
                    <option value="crashed">Crashed</option>
                    <option value="installing">Installing</option>
                    <option value="updating">Updating</option>
                    <option value="uninstalling">Uninstalling</option>
                  </select>
                </div>
                <div class="sidebar-filter-group">
                  <label class="sidebar-label">Per Page</label>
                  <select
                    class="per-page-selector"
                    value={perPage()}
                    onChange={(e) =>
                      handlePerPageChange(parseInt(e.currentTarget.value, 10))
                    }
                  >
                    <option value="10">10 per page</option>
                    <option value="25">25 per page</option>
                    <option value="50">50 per page</option>
                    <option value="100">100 per page</option>
                  </select>
                </div>
                <Show when={search() || status()}>
                  <button
                    class="btn btn-sm btn-secondary sidebar-clear-btn"
                    onClick={() => {
                      setSearchInput("");
                      setSearchParams({
                        search: undefined,
                        status: undefined,
                        page: "1",
                      });
                    }}
                  >
                    Clear Filters
                  </button>
                </Show>
              </div>
            </div>
          </aside>
        </Show>

        <div class="dashboard-main">
          <Show when={data.loading && !data()}>
            <Loader message="Loading servers" />
          </Show>

          <Show when={data.error}>
            <div class="error-msg">
              Failed to load servers:{" "}
              {String(data.error?.message ?? data.error)}
            </div>
          </Show>

          <Show when={data()}>
            {(resolved) => (
              <>
                <div class="pagination-info">
                  Showing {resolved().servers.length} of{" "}
                  {Number(resolved().total)} servers
                  {search() && ` matching "${search()}"`}
                  {status() && ` with status "${status()}"`}
                </div>

                <Show
                  when={resolved().servers.length > 0}
                  fallback={
                    <div class="empty-state">
                      <Show
                        when={search() || status()}
                        fallback={
                          <>
                            <h2>No servers configured yet</h2>
                            <p>
                              Create your first server to start managing
                              processes with AnyServer.
                            </p>
                            <A href="/create" class="btn btn-primary">
                              Create your first server
                            </A>
                          </>
                        }
                      >
                        <h2>No servers match your filters</h2>
                        <p>Try adjusting your search or filter criteria.</p>
                        <button
                          class="btn btn-secondary"
                          onClick={() => {
                            setSearchInput("");
                            setSearchParams({
                              search: undefined,
                              status: undefined,
                              page: "1",
                            });
                          }}
                        >
                          Clear Filters
                        </button>
                      </Show>
                    </div>
                  }
                >
                  <div class="server-grid">
                    <For each={resolved().servers}>
                      {(server) => (
                        <ServerCard
                          server={patchServer(server)}
                          onAction={refetch}
                          updateResult={
                            updateResults()[server.server.id] ?? null
                          }
                        />
                      )}
                    </For>
                  </div>

                  <Show when={resolved().total_pages > 1}>
                    <div class="pagination-controls">
                      <button
                        class="btn btn-secondary"
                        disabled={page() <= 1}
                        onClick={() => handlePageChange(page() - 1)}
                      >
                        Previous
                      </button>
                      <span class="pagination-info-text">
                        Page {page()} of {resolved().total_pages}
                      </span>
                      <button
                        class="btn btn-secondary"
                        disabled={page() >= resolved().total_pages}
                        onClick={() => handlePageChange(page() + 1)}
                      >
                        Next
                      </button>
                    </div>
                  </Show>
                </Show>
              </>
            )}
          </Show>
        </div>
      </div>
    </div>
  );
};

export default Dashboard;
