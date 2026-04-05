import { type Component, createSignal, createEffect, Show } from "solid-js";
import { useNavigate } from "@solidjs/router";
import Loader from "../components/Loader";
import { useAuth } from "../context/auth";
import UsersTab from "../components/admin/UsersTab";
import InviteCodesTab from "../components/admin/InviteCodesTab";
import UserPermissionsTab from "../components/admin/UserPermissionsTab";
import SettingsTab from "../components/admin/SettingsTab";
import SmtpTab from "../components/admin/SmtpTab";
import AlertsTab from "../components/admin/AlertsTab";
import PasswordTab from "../components/admin/PasswordTab";
import SessionsTab from "../components/admin/SessionsTab";
import GithubTab from "../components/admin/GithubTab";
import CurseForgeTab from "../components/admin/CurseForgeTab";
import SandboxFeatureTab from "../components/admin/SandboxFeatureTab";

type Tab =
  | "users"
  | "invites"
  | "permissions"
  | "settings"
  | "password"
  | "smtp"
  | "alerts"
  | "sessions"
  | "github"
  | "curseforge"
  | "sandbox";

const AdminPanel: Component = () => {
  const navigate = useNavigate();
  const auth = useAuth();

  // Reactively redirect non-admins once auth has finished loading.
  // This must be an effect (not an imperative check) because on a full
  // page refresh the auth context is still loading when the component
  // first mounts — `user()` is null until the token refresh completes.
  createEffect(() => {
    if (!auth.loading() && !auth.isAdmin()) {
      navigate("/", { replace: true });
    }
  });

  const [tab, setTab] = createSignal<Tab>("users");

  return (
    <Show when={!auth.loading() && auth.isAdmin()} fallback={<Loader />}>
      <div class="admin-panel">
        <div class="page-header">
          <h1>Admin Panel</h1>
        </div>

        <div class="tabs">
          <button
            class={`tab ${tab() === "users" ? "active" : ""}`}
            onClick={() => setTab("users")}
          >
            Users
          </button>
          <button
            class={`tab ${tab() === "invites" ? "active" : ""}`}
            onClick={() => setTab("invites")}
          >
            Invite Codes
          </button>
          <button
            class={`tab ${tab() === "permissions" ? "active" : ""}`}
            onClick={() => setTab("permissions")}
          >
            Permissions
          </button>
          <button
            class={`tab ${tab() === "settings" ? "active" : ""}`}
            onClick={() => setTab("settings")}
          >
            Settings
          </button>
          <button
            class={`tab ${tab() === "smtp" ? "active" : ""}`}
            onClick={() => setTab("smtp")}
          >
            Email (SMTP)
          </button>
          <button
            class={`tab ${tab() === "alerts" ? "active" : ""}`}
            onClick={() => setTab("alerts")}
          >
            Alerts
          </button>
          <button
            class={`tab ${tab() === "password" ? "active" : ""}`}
            onClick={() => setTab("password")}
          >
            Change Password
          </button>
          <button
            class={`tab ${tab() === "sessions" ? "active" : ""}`}
            onClick={() => setTab("sessions")}
          >
            Sessions
          </button>
          <button
            class={`tab ${tab() === "github" ? "active" : ""}`}
            onClick={() => setTab("github")}
          >
            GitHub
          </button>
          <button
            class={`tab ${tab() === "curseforge" ? "active" : ""}`}
            onClick={() => setTab("curseforge")}
          >
            CurseForge
          </button>
          <button
            class={`tab ${tab() === "sandbox" ? "active" : ""}`}
            onClick={() => setTab("sandbox")}
          >
            Sandbox
          </button>
        </div>

        <div class="tab-content">
          <Show when={tab() === "users"}>
            <UsersTab />
          </Show>
          <Show when={tab() === "invites"}>
            <InviteCodesTab />
          </Show>
          <Show when={tab() === "permissions"}>
            <UserPermissionsTab />
          </Show>
          <Show when={tab() === "settings"}>
            <SettingsTab />
          </Show>
          <Show when={tab() === "smtp"}>
            <SmtpTab />
          </Show>
          <Show when={tab() === "alerts"}>
            <AlertsTab />
          </Show>
          <Show when={tab() === "password"}>
            <PasswordTab />
          </Show>
          <Show when={tab() === "sessions"}>
            <SessionsTab />
          </Show>
          <Show when={tab() === "github"}>
            <GithubTab />
          </Show>
          <Show when={tab() === "curseforge"}>
            <CurseForgeTab />
          </Show>
          <Show when={tab() === "sandbox"}>
            <SandboxFeatureTab />
          </Show>
        </div>
      </div>
    </Show>
  );
};

export default AdminPanel;
