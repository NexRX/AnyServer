import type { ServerConfig } from "../../types/bindings";

export type WizardStepId =
  | "parameters"
  | "basics"
  | "start"
  | "install"
  | "update"
  | "review";

export interface WizardStepDef {
  id: WizardStepId;
  label: string;
  icon: string;
  description: string;
}

export const WIZARD_STEPS: WizardStepDef[] = [
  {
    id: "parameters",
    label: "Parameters",
    icon: "🔧",
    description:
      "Define template parameters referenced as ${name} throughout your config and pipelines.",
  },
  {
    id: "basics",
    label: "Server Info",
    icon: "📋",
    description: "Name your server and set the working directory.",
  },
  {
    id: "start",
    label: "Start Command",
    icon: "▶️",
    description: "Configure how the server process launches and stops.",
  },
  {
    id: "install",
    label: "Install Steps",
    icon: "📦",
    description:
      "Define the pipeline that runs on first-time setup (download files, extract, configure, etc.).",
  },
  {
    id: "update",
    label: "Update Steps",
    icon: "🔄",
    description: "Define the pipeline that runs when updating the server.",
  },
  {
    id: "review",
    label: "Review & Create",
    icon: "✅",
    description: "Review your configuration and fill in parameter values.",
  },
];

export const defaultConfig: ServerConfig = {
  name: "New Server",
  binary: "",
  args: [],
  env: {},
  working_dir: null,
  auto_start: false,
  auto_restart: false,
  max_restart_attempts: 0,
  restart_delay_secs: 5,
  stop_command: null,
  stop_signal: "sigterm",
  stop_timeout_secs: 10,
  stop_steps: [],
  sftp_username: null,
  sftp_password: null,
  parameters: [],
  start_steps: [],
  install_steps: [],
  update_steps: [],
  uninstall_steps: [],
  isolation: {
    enabled: true,
    extra_read_paths: [],
    extra_rw_paths: [],
    pids_max: null,
  },
  update_check: null,
  log_to_disk: true,
  max_log_size_mb: 50,
  enable_java_helper: false,
  enable_dotnet_helper: false,
  steam_app_id: null,
};
