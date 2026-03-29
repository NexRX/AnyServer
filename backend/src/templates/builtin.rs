//! Built-in curated server templates shipped with AnyServer.
//!
//! These templates are served from memory and never touch the database.
//! They cannot be modified or deleted by users.

use chrono::{DateTime, TimeZone, Utc};
use std::collections::HashMap;
use std::sync::LazyLock;
use uuid::Uuid;

use crate::types::pipeline::*;
use crate::types::server::*;
use crate::types::template::ServerTemplate;

// ─── Helper: PipelineStep builder ───
// Reduces boilerplate when constructing steps with default values.

fn step_desc(name: impl Into<String>, desc: impl Into<String>, action: StepAction) -> PipelineStep {
    PipelineStep {
        name: name.into(),
        description: Some(desc.into()),
        action,
        condition: None,
        continue_on_error: false,
    }
}

fn step_desc_cond(
    name: impl Into<String>,
    desc: impl Into<String>,
    action: StepAction,
    condition: StepCondition,
) -> PipelineStep {
    PipelineStep {
        name: name.into(),
        description: Some(desc.into()),
        action,
        condition: Some(condition),
        continue_on_error: false,
    }
}

// ─── Helper: Paper download steps ───
// The PaperMC v2 API requires two lookups to download a jar:
//   1. Resolve the latest build number for a given version.
//   2. Download using the build number in the URL.
// We package these as a helper so install_steps and update_steps stay DRY.

fn paper_download_steps() -> Vec<PipelineStep> {
    vec![
        step_desc(
            "Resolve latest Paper build",
            "Query the PaperMC API for the latest build number.",
            StepAction::ResolveVariable {
                url: "https://api.papermc.io/v2/projects/paper/versions/${mc_version}/builds".into(),
                path: Some("builds".into()),
                pick: VersionPick::Last,
                value_key: Some("build".into()),
                variable: "paper_build".into(),
            },
        ),
        step_desc(
            "Download Paper",
            "Download the Paper server jar from the PaperMC API.",
            StepAction::Download {
                url: "https://api.papermc.io/v2/projects/paper/versions/${mc_version}/builds/${paper_build}/downloads/paper-${mc_version}-${paper_build}.jar".into(),
                destination: ".".into(),
                filename: Some("paper.jar".into()),
                executable: false,
            },
        ),
    ]
}

// ─── Fixed UUIDs for built-in templates ───
// Generated deterministically so they stay stable across restarts.

const MINECRAFT_PAPER_UUID: Uuid = Uuid::from_bytes([
    0x00, 0xba, 0xfe, 0xed, 0x00, 0x01, 0x40, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
]);

const VALHEIM_UUID: Uuid = Uuid::from_bytes([
    0x00, 0xba, 0xfe, 0xed, 0x00, 0x01, 0x40, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02,
]);

const TERRARIA_TSHOCK_UUID: Uuid = Uuid::from_bytes([
    0x00, 0xba, 0xfe, 0xed, 0x00, 0x01, 0x40, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03,
]);

/// A sentinel "system" user UUID used as the `created_by` for built-in
/// templates.  This UUID will never collide with a real user.
const SYSTEM_USER_UUID: Uuid = Uuid::from_bytes([
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
]);

/// Epoch used for `created_at` / `updated_at` on built-in templates.
fn builtin_timestamp() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap()
}

// ─── Template Definitions ───

fn minecraft_paper_template() -> ServerTemplate {
    ServerTemplate {
        id: MINECRAFT_PAPER_UUID,
        name: "Minecraft Paper".into(),
        description: Some(
            "High-performance Minecraft server based on Paper. Supports plugins, \
             automatic EULA acceptance, and configurable memory allocation. \
             Requires Java 21+ to be installed on the host."
                .into(),
        ),
        config: ServerConfig {
            name: "Minecraft Paper Server".into(),
            binary: "java".into(),
            args: vec![
                "-Xms${memory}".into(),
                "-Xmx${memory}".into(),
                "-jar".into(),
                "paper.jar".into(),
                "--nogui".into(),
                "--port".into(),
                "${server_port}".into(),
            ],
            env: HashMap::new(),
            working_dir: None,
            auto_start: false,
            auto_restart: true,
            max_restart_attempts: 3,
            restart_delay_secs: 10,
            stop_command: Some("stop".into()),
            stop_signal: StopSignal::Sigterm,
            stop_timeout_secs: 30,
            sftp_username: None,
            sftp_password: None,
            parameters: vec![
                ConfigParameter {
                    name: "mc_version".into(),
                    label: "Minecraft Version".into(),
                    description: Some("The Minecraft version to install (e.g. 1.21.4).".into()),
                    default: Some("1.21.4".into()),
                    required: true,
                    regex: Some(r"^\d+\.\d+(\.\d+)?$".into()),
                    is_version: true,
                    options_from: Some(OptionsFrom {
                        url: "https://api.papermc.io/v2/projects/paper".into(),
                        path: Some("versions".into()),
                        value_key: None,
                        label_key: None,
                        sort: Some(OptionsSortOrder::Desc),
                        limit: Some(25),
                        cache_secs: Some(300),
                    }),
                    ..Default::default()
                },
                ConfigParameter {
                    name: "memory".into(),
                    label: "Memory Allocation".into(),
                    description: Some(
                        "JVM heap size (e.g. 2G, 4G). Applied to both -Xms and -Xmx.".into(),
                    ),
                    param_type: ConfigParameterType::Select,
                    default: Some("2G".into()),
                    required: true,
                    options: vec![
                        "1G".into(),
                        "2G".into(),
                        "4G".into(),
                        "6G".into(),
                        "8G".into(),
                    ],
                    ..Default::default()
                },
                ConfigParameter {
                    name: "server_port".into(),
                    label: "Server Port".into(),
                    description: Some("The port the server listens on.".into()),
                    param_type: ConfigParameterType::Number,
                    default: Some("25565".into()),
                    required: true,
                    ..Default::default()
                },
            ],
            stop_steps: vec![
                step_desc(
                    "Send stop command",
                    "Send the 'stop' command to gracefully shut down.",
                    StepAction::SendInput {
                        text: "stop\n".into(),
                    },
                ),
                PipelineStep {
                    name: "Wait for shutdown".into(),
                    description: Some("Wait for the server to finish saving.".into()),
                    action: StepAction::WaitForOutput {
                        pattern: "All dimensions are saved".into(),
                        timeout_secs: 30,
                    },
                    condition: None,
                    continue_on_error: true,
                },
            ],
            start_steps: vec![],
            install_steps: {
                let mut steps = paper_download_steps();
                steps.extend(vec![
                    step_desc(
                        "Accept EULA",
                        "Write eula.txt to accept the Minecraft EULA.",
                        StepAction::WriteFile {
                            path: "eula.txt".into(),
                            content: "# Auto-accepted by AnyServer template\neula=true\n".into(),
                        },
                    ),
                    step_desc_cond(
                        "Write server.properties",
                        "Create a default server.properties with the configured port.",
                        StepAction::WriteFile {
                            path: "server.properties".into(),
                            content: "# Minecraft Server Properties\n# Generated by AnyServer\nserver-port=${server_port}\nonline-mode=true\nmax-players=20\nview-distance=10\nspawn-protection=0\n".into(),
                        },
                        StepCondition {
                            path_exists: None,
                            path_not_exists: Some("server.properties".into()),
                        },
                    ),
                ]);
                steps
            },
            update_steps: paper_download_steps(),
            uninstall_steps: vec![],
            isolation: IsolationConfig::default(),
            update_check: Some(UpdateCheck {
                provider: UpdateCheckProvider::Api {
                    url: "https://api.papermc.io/v2/projects/paper".into(),
                    path: Some("versions".into()),
                    pick: VersionPick::Last,
                    value_key: None,
                },
                interval_secs: Some(3600),
                cache_secs: 300,
            }),
            log_to_disk: true,
            max_log_size_mb: 50,
            enable_java_helper: true,
            enable_dotnet_helper: false,
            steam_app_id: None,
        },
        created_by: SYSTEM_USER_UUID,
        created_at: builtin_timestamp(),
        updated_at: builtin_timestamp(),
        is_builtin: true,
        requires_steamcmd: false,
    }
}

fn valheim_template() -> ServerTemplate {
    ServerTemplate {
        id: VALHEIM_UUID,
        name: "Valheim Dedicated Server".into(),
        description: Some(
            "Valheim dedicated server installed via Steam tools. \
             Requires Steam command-line tooling to be available on PATH. \
             The server is identified by Steam App ID 896660."
                .into(),
        ),
        config: ServerConfig {
            name: "Valheim Server".into(),
            binary: "./valheim_server.x86_64".into(),
            args: vec![
                "-name".into(),
                "${server_name}".into(),
                "-port".into(),
                "${server_port}".into(),
                "-world".into(),
                "${world_name}".into(),
                "-password".into(),
                "${password}".into(),
                "-public".into(),
                "1".into(),
            ],
            env: {
                let mut env = HashMap::new();
                env.insert(
                    "LD_LIBRARY_PATH".into(),
                    "./linux64:$LD_LIBRARY_PATH".into(),
                );
                env.insert("SteamAppId".into(), "892970".into());
                env
            },
            working_dir: None,
            auto_start: false,
            auto_restart: true,
            max_restart_attempts: 3,
            restart_delay_secs: 15,
            stop_command: None,
            stop_signal: StopSignal::Sigint,
            stop_timeout_secs: 30,
            sftp_username: None,
            sftp_password: None,
            parameters: vec![
                ConfigParameter {
                    name: "server_name".into(),
                    label: "Server Name".into(),
                    description: Some(
                        "The public name shown in the Valheim server browser.".into(),
                    ),
                    default: Some("My Valheim Server".into()),
                    required: true,
                    ..Default::default()
                },
                ConfigParameter {
                    name: "world_name".into(),
                    label: "World Name".into(),
                    description: Some("Name of the world save file.".into()),
                    default: Some("Dedicated".into()),
                    required: true,
                    ..Default::default()
                },
                ConfigParameter {
                    name: "password".into(),
                    label: "Server Password".into(),
                    description: Some(
                        "Password required to join (must be at least 5 characters).".into(),
                    ),
                    required: true,
                    regex: Some(r"^.{5,}$".into()),
                    ..Default::default()
                },
                ConfigParameter {
                    name: "server_port".into(),
                    label: "Server Port".into(),
                    description: Some("Base UDP port. Valheim uses this port and port+1.".into()),
                    param_type: ConfigParameterType::Number,
                    default: Some("2456".into()),
                    required: true,
                    ..Default::default()
                },
            ],
            stop_steps: vec![
                step_desc(
                    "Send SIGINT",
                    "Valheim handles SIGINT for graceful world save and shutdown.",
                    StepAction::SendSignal {
                        signal: StopSignal::Sigint,
                    },
                ),
                PipelineStep {
                    name: "Wait for shutdown".into(),
                    description: Some("Wait for the server process to exit cleanly.".into()),
                    action: StepAction::Sleep { seconds: 10 },
                    condition: None,
                    continue_on_error: true,
                },
            ],
            start_steps: vec![step_desc(
                "Set library path",
                "Ensure the Valheim shared libraries are on LD_LIBRARY_PATH.",
                StepAction::SetEnv {
                    variables: {
                        let mut vars = HashMap::new();
                        vars.insert(
                            "LD_LIBRARY_PATH".into(),
                            "./linux64:$LD_LIBRARY_PATH".into(),
                        );
                        vars.insert("SteamAppId".into(), "892970".into());
                        vars
                    },
                },
            )],
            install_steps: vec![
                step_desc(
                    "Install Valheim server",
                    "Use SteamCMD to download/install Valheim Dedicated Server (App 896660).",
                    StepAction::SteamCmdInstall {
                        app_id: None, // uses steam_app_id from config
                        anonymous: true,
                        extra_args: vec![],
                    },
                ),
                PipelineStep {
                    name: "Make server executable".into(),
                    description: Some("Ensure the server binary has execute permission.".into()),
                    action: StepAction::SetPermissions {
                        path: "valheim_server.x86_64".into(),
                        mode: "755".into(),
                    },
                    condition: Some(StepCondition {
                        path_exists: Some("valheim_server.x86_64".into()),
                        path_not_exists: None,
                    }),
                    continue_on_error: true,
                },
            ],
            update_steps: vec![step_desc(
                "Update Valheim server",
                "Use SteamCMD to update to the latest version.",
                StepAction::SteamCmdUpdate {
                    app_id: None, // uses steam_app_id from config
                    anonymous: true,
                    extra_args: vec![],
                },
            )],
            uninstall_steps: vec![],
            isolation: IsolationConfig::default(),
            update_check: None,
            log_to_disk: true,
            max_log_size_mb: 50,
            enable_java_helper: false,
            enable_dotnet_helper: false,
            steam_app_id: Some(896660),
        },
        created_by: SYSTEM_USER_UUID,
        created_at: builtin_timestamp(),
        updated_at: builtin_timestamp(),
        is_builtin: true,
        requires_steamcmd: true,
    }
}

fn terraria_tshock_template() -> ServerTemplate {
    ServerTemplate {
        id: TERRARIA_TSHOCK_UUID,
        name: "Terraria (TShock)".into(),
        description: Some(
            "Terraria server powered by TShock — a popular server mod with admin tools, \
             anti-cheat, and plugin support. Downloads the TShock release archive and \
             extracts it.\n\n\
             **Important**: This template requires .NET 6.0+ runtime. When creating your \
             server, you'll see a .NET Runtime selector — click \"Detect .NET Runtimes\" \
             and select an appropriate version to automatically configure the required \
             environment variables."
                .into(),
        ),
        config: ServerConfig {
            name: "Terraria TShock Server".into(),
            binary: "./TShock.Server".into(),
            args: vec![
                "-port".into(),
                "${server_port}".into(),
                "-maxplayers".into(),
                "${max_players}".into(),
                "-world".into(),
                "worlds/${world_name}.wld".into(),
                "-autocreate".into(),
                "2".into(),
            ],
            env: HashMap::new(),
            working_dir: None,
            auto_start: false,
            auto_restart: true,
            max_restart_attempts: 3,
            restart_delay_secs: 10,
            stop_command: Some("exit".into()),
            stop_signal: StopSignal::Sigterm,
            stop_timeout_secs: 15,
            sftp_username: None,
            sftp_password: None,
            parameters: vec![
                ConfigParameter {
                    name: "tshock_version".into(),
                    label: "TShock Version".into(),
                    description: Some("Select a TShock release tag to install.".into()),
                    param_type: ConfigParameterType::GithubReleaseTag,
                    default: None,
                    required: true,
                    is_version: true,
                    github_repo: Some("Pryaxis/TShock".into()),
                    ..Default::default()
                },
                ConfigParameter {
                    name: "max_players".into(),
                    label: "Max Players".into(),
                    description: Some("Maximum number of players that can connect.".into()),
                    param_type: ConfigParameterType::Select,
                    default: Some("8".into()),
                    required: true,
                    options: vec![
                        "4".into(),
                        "8".into(),
                        "16".into(),
                        "32".into(),
                        "64".into(),
                    ],
                    ..Default::default()
                },
                ConfigParameter {
                    name: "server_port".into(),
                    label: "Server Port".into(),
                    description: Some("The TCP port the Terraria server listens on.".into()),
                    param_type: ConfigParameterType::Number,
                    default: Some("7777".into()),
                    required: true,
                    ..Default::default()
                },
                ConfigParameter {
                    name: "world_name".into(),
                    label: "World Name".into(),
                    description: Some(
                        "Name of the world file. A new world is auto-created if it doesn't exist."
                            .into(),
                    ),
                    default: Some("world".into()),
                    required: true,
                    regex: Some(r"^[a-zA-Z0-9_-]+$".into()),
                    ..Default::default()
                },
            ],
            stop_steps: vec![
                step_desc(
                    "Send exit command",
                    "Send the 'exit' command to save and shut down gracefully.",
                    StepAction::SendInput {
                        text: "exit\n".into(),
                    },
                ),
                PipelineStep {
                    name: "Wait for save".into(),
                    description: Some("Wait for TShock to finish saving.".into()),
                    action: StepAction::Sleep { seconds: 5 },
                    condition: None,
                    continue_on_error: true,
                },
            ],
            start_steps: vec![],
            install_steps: vec![
                PipelineStep {
                    name: "Create worlds directory".into(),
                    description: Some("Ensure the worlds directory exists.".into()),
                    action: StepAction::CreateDir {
                        path: "worlds".into(),
                    },
                    condition: None,
                    continue_on_error: true,
                },
                step_desc(
                    "Download TShock",
                    "Download the TShock release zip from GitHub.",
                    StepAction::DownloadGithubReleaseAsset {
                        tag_param: "tshock_version".into(),
                        asset_matcher: "/TShock-.*-for-Terraria-.*-linux-x64-Release\\.zip$/"
                            .into(),
                        destination: ".".into(),
                        filename: Some("tshock.zip".into()),
                        executable: false,
                    },
                ),
                step_desc(
                    "Extract zip archive",
                    "Extract the TShock zip to get the tar file.",
                    StepAction::Extract {
                        source: "tshock.zip".into(),
                        destination: Some(".".into()),
                        format: ArchiveFormat::Zip,
                    },
                ),
                PipelineStep {
                    name: "Clean up zip".into(),
                    description: Some("Remove the downloaded zip file.".into()),
                    action: StepAction::Delete {
                        path: "tshock.zip".into(),
                        recursive: false,
                    },
                    condition: None,
                    continue_on_error: true,
                },
                step_desc(
                    "Extract TShock tar",
                    "Extract the TShock tar archive containing the server files.",
                    StepAction::Extract {
                        source: "TShock-Beta-linux-x64-Release.tar".into(),
                        destination: Some(".".into()),
                        format: ArchiveFormat::Tar,
                    },
                ),
                PipelineStep {
                    name: "Clean up tar".into(),
                    description: Some("Remove the extracted tar file.".into()),
                    action: StepAction::Delete {
                        path: "TShock-Beta-linux-x64-Release.tar".into(),
                        recursive: false,
                    },
                    condition: None,
                    continue_on_error: true,
                },
                PipelineStep {
                    name: "Make server executable".into(),
                    description: Some(
                        "Ensure the TShock server binary has execute permission.".into(),
                    ),
                    action: StepAction::SetPermissions {
                        path: "TShock.Server".into(),
                        mode: "755".into(),
                    },
                    condition: Some(StepCondition {
                        path_exists: Some("TShock.Server".into()),
                        path_not_exists: None,
                    }),
                    continue_on_error: true,
                },
            ],
            update_steps: vec![
                step_desc(
                    "Download TShock",
                    "Download the latest TShock release zip from GitHub.",
                    StepAction::DownloadGithubReleaseAsset {
                        tag_param: "tshock_version".into(),
                        asset_matcher: "/TShock-.*-for-Terraria-.*-linux-x64-Release\\.zip$/"
                            .into(),
                        destination: ".".into(),
                        filename: Some("tshock.zip".into()),
                        executable: false,
                    },
                ),
                step_desc(
                    "Extract zip archive",
                    "Extract the TShock zip to get the tar file.",
                    StepAction::Extract {
                        source: "tshock.zip".into(),
                        destination: Some(".".into()),
                        format: ArchiveFormat::Zip,
                    },
                ),
                PipelineStep {
                    name: "Clean up zip".into(),
                    description: Some("Remove the downloaded zip file.".into()),
                    action: StepAction::Delete {
                        path: "tshock.zip".into(),
                        recursive: false,
                    },
                    condition: None,
                    continue_on_error: true,
                },
                step_desc(
                    "Extract TShock tar",
                    "Extract the updated TShock tar archive containing the server files.",
                    StepAction::Extract {
                        source: "TShock-Beta-linux-x64-Release.tar".into(),
                        destination: Some(".".into()),
                        format: ArchiveFormat::Tar,
                    },
                ),
                PipelineStep {
                    name: "Clean up tar".into(),
                    description: Some("Remove the extracted tar file.".into()),
                    action: StepAction::Delete {
                        path: "TShock-Beta-linux-x64-Release.tar".into(),
                        recursive: false,
                    },
                    condition: None,
                    continue_on_error: true,
                },
                PipelineStep {
                    name: "Make server executable".into(),
                    description: Some(
                        "Ensure the TShock server binary has execute permission after update."
                            .into(),
                    ),
                    action: StepAction::SetPermissions {
                        path: "TShock.Server".into(),
                        mode: "755".into(),
                    },
                    condition: Some(StepCondition {
                        path_exists: Some("TShock.Server".into()),
                        path_not_exists: None,
                    }),
                    continue_on_error: true,
                },
            ],
            uninstall_steps: vec![],
            isolation: IsolationConfig::default(),
            update_check: Some(UpdateCheck {
                provider: UpdateCheckProvider::Api {
                    url: "https://api.github.com/repos/Pryaxis/TShock/releases".into(),
                    path: None,
                    pick: VersionPick::First,
                    value_key: Some("tag_name".into()),
                },
                interval_secs: Some(7200),
                cache_secs: 600,
            }),
            log_to_disk: true,
            max_log_size_mb: 50,
            enable_java_helper: false,
            enable_dotnet_helper: true,
            steam_app_id: None,
        },
        created_by: SYSTEM_USER_UUID,
        created_at: builtin_timestamp(),
        updated_at: builtin_timestamp(),
        is_builtin: true,
        requires_steamcmd: false,
    }
}

static BUILTIN_TEMPLATES: LazyLock<Vec<ServerTemplate>> = LazyLock::new(|| {
    vec![
        minecraft_paper_template(),
        valheim_template(),
        terraria_tshock_template(),
    ]
});

/// Returns all built-in templates.
pub fn list() -> &'static [ServerTemplate] {
    &BUILTIN_TEMPLATES
}

/// Look up a built-in template by ID.  Returns `None` if the ID doesn't
/// match any built-in template.
pub fn get(id: uuid::Uuid) -> Option<&'static ServerTemplate> {
    BUILTIN_TEMPLATES.iter().find(|t| t.id == id)
}

/// Returns `true` if the given UUID belongs to a built-in template.
pub fn is_builtin(id: uuid::Uuid) -> bool {
    BUILTIN_TEMPLATES.iter().any(|t| t.id == id)
}

// ─── Tests ───

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_templates_have_unique_ids() {
        let templates = list();
        let mut ids: Vec<Uuid> = templates.iter().map(|t| t.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(
            ids.len(),
            templates.len(),
            "built-in template IDs must be unique"
        );
    }

    #[test]
    fn builtin_templates_are_marked_builtin() {
        for t in list() {
            assert!(
                t.is_builtin,
                "template '{}' must have is_builtin=true",
                t.name
            );
        }
    }

    #[test]
    fn builtin_templates_have_valid_configs() {
        for t in list() {
            assert!(!t.name.is_empty(), "template name must not be empty");
            assert!(
                t.description.is_some(),
                "built-in template '{}' should have a description",
                t.name
            );
            assert!(
                !t.config.binary.is_empty(),
                "built-in template '{}' must have a binary",
                t.name
            );
            assert!(
                !t.config.install_steps.is_empty(),
                "built-in template '{}' must have install steps",
                t.name
            );
            assert!(
                !t.config.parameters.is_empty(),
                "built-in template '{}' must have parameters",
                t.name
            );
        }
    }

    #[test]
    fn get_returns_correct_template() {
        let mc = get(MINECRAFT_PAPER_UUID);
        assert!(mc.is_some());
        assert_eq!(mc.unwrap().name, "Minecraft Paper");

        let valheim = get(VALHEIM_UUID);
        assert!(valheim.is_some());
        assert_eq!(valheim.unwrap().name, "Valheim Dedicated Server");

        let terraria = get(TERRARIA_TSHOCK_UUID);
        assert!(terraria.is_some());
        assert_eq!(terraria.unwrap().name, "Terraria (TShock)");
    }

    #[test]
    fn get_returns_none_for_unknown_id() {
        let unknown = Uuid::from_bytes([0xFF; 16]);
        assert!(get(unknown).is_none());
    }

    #[test]
    fn is_builtin_works() {
        assert!(is_builtin(MINECRAFT_PAPER_UUID));
        assert!(is_builtin(VALHEIM_UUID));
        assert!(is_builtin(TERRARIA_TSHOCK_UUID));

        let unknown = Uuid::from_bytes([0xFF; 16]);
        assert!(!is_builtin(unknown));
    }

    #[test]
    fn list_returns_at_least_three_templates() {
        assert!(list().len() >= 3, "must ship at least 3 built-in templates");
    }

    #[test]
    fn templates_cover_different_types() {
        // Verify we have a Java-based, a SteamCMD-based, and an archive-based template
        let templates = list();

        let has_java = templates.iter().any(|t| t.config.binary == "java");
        assert!(
            has_java,
            "should have a Java-based template (Minecraft Paper)"
        );

        let has_steamcmd = templates.iter().any(|t| {
            t.config.install_steps.iter().any(|s| {
                matches!(
                    &s.action,
                    StepAction::SteamCmdInstall { .. } | StepAction::SteamCmdUpdate { .. }
                )
            })
        });
        assert!(
            has_steamcmd,
            "should have a SteamCMD-based template (Valheim)"
        );

        let has_archive_extract = templates.iter().any(|t| {
            t.config
                .install_steps
                .iter()
                .any(|s| matches!(&s.action, StepAction::Extract { .. }))
        });
        assert!(
            has_archive_extract,
            "should have a template that extracts an archive (Terraria TShock)"
        );
    }

    #[test]
    fn minecraft_template_has_eula_step() {
        let mc = get(MINECRAFT_PAPER_UUID).unwrap();
        let has_eula = mc.config.install_steps.iter().any(|s| {
            if let StepAction::WriteFile { path, content } = &s.action {
                path.contains("eula") && content.contains("eula=true")
            } else {
                false
            }
        });
        assert!(has_eula, "Minecraft template must auto-accept EULA");
    }

    #[test]
    fn all_required_params_have_defaults_or_are_documented() {
        for t in list() {
            for p in &t.config.parameters {
                if p.required {
                    // Required params should either have a default or a description
                    // explaining what to fill in
                    assert!(
                        p.default.is_some() || p.description.is_some(),
                        "Required parameter '{}' in template '{}' should have a default or description",
                        p.name,
                        t.name
                    );
                }
            }
        }
    }

    // ── Variable reference integrity ─────────────────────────────────────
    //
    // Every `${var}` reference in args, step URLs, step content, etc. must
    // either come from a parameter definition OR be produced by a prior
    // ResolveVariable step in the same pipeline.  Unreferenced variables
    // silently stay as literal `${...}` strings at runtime, which is a
    // guaranteed broken deployment.

    /// Extract all `${name}` references from a string.
    fn extract_var_refs(s: &str) -> Vec<String> {
        let mut refs = Vec::new();
        let mut rest = s;
        while let Some(start) = rest.find("${") {
            if let Some(end) = rest[start..].find('}') {
                let name = &rest[start + 2..start + end];
                if !name.is_empty() {
                    refs.push(name.to_string());
                }
                rest = &rest[start + end + 1..];
            } else {
                break;
            }
        }
        refs
    }

    /// Collect all `${var}` references from a list of pipeline steps.
    fn collect_step_var_refs(steps: &[PipelineStep]) -> Vec<(String, String)> {
        let mut refs = Vec::new();
        for step in steps {
            let step_name = &step.name;
            match &step.action {
                StepAction::Download {
                    url,
                    destination,
                    filename,
                    ..
                } => {
                    for v in extract_var_refs(url) {
                        refs.push((step_name.clone(), v));
                    }
                    for v in extract_var_refs(destination) {
                        refs.push((step_name.clone(), v));
                    }
                    if let Some(f) = filename {
                        for v in extract_var_refs(f) {
                            refs.push((step_name.clone(), v));
                        }
                    }
                }
                StepAction::WriteFile { path, content } => {
                    for v in extract_var_refs(path) {
                        refs.push((step_name.clone(), v));
                    }
                    for v in extract_var_refs(content) {
                        refs.push((step_name.clone(), v));
                    }
                }
                StepAction::RunCommand { command, args, .. } => {
                    for v in extract_var_refs(command) {
                        refs.push((step_name.clone(), v));
                    }
                    for arg in args {
                        for v in extract_var_refs(arg) {
                            refs.push((step_name.clone(), v));
                        }
                    }
                }
                StepAction::Extract {
                    source,
                    destination,
                    ..
                } => {
                    for v in extract_var_refs(source) {
                        refs.push((step_name.clone(), v));
                    }
                    if let Some(d) = destination {
                        for v in extract_var_refs(d) {
                            refs.push((step_name.clone(), v));
                        }
                    }
                }
                StepAction::Delete { path, .. } => {
                    for v in extract_var_refs(path) {
                        refs.push((step_name.clone(), v));
                    }
                }
                StepAction::CreateDir { path } => {
                    for v in extract_var_refs(path) {
                        refs.push((step_name.clone(), v));
                    }
                }
                StepAction::SetPermissions { path, mode } => {
                    for v in extract_var_refs(path) {
                        refs.push((step_name.clone(), v));
                    }
                    for v in extract_var_refs(mode) {
                        refs.push((step_name.clone(), v));
                    }
                }
                StepAction::EditFile { path, .. } => {
                    for v in extract_var_refs(path) {
                        refs.push((step_name.clone(), v));
                    }
                }
                StepAction::ResolveVariable { url, variable, .. } => {
                    for v in extract_var_refs(url) {
                        refs.push((step_name.clone(), v));
                    }
                    // The variable produced by this step is NOT a reference;
                    // it's an output.  We don't add it here.
                    let _ = variable;
                }
                StepAction::SendInput { text } => {
                    for v in extract_var_refs(text) {
                        refs.push((step_name.clone(), v));
                    }
                }
                StepAction::Glob {
                    pattern,
                    destination,
                } => {
                    for v in extract_var_refs(pattern) {
                        refs.push((step_name.clone(), v));
                    }
                    for v in extract_var_refs(destination) {
                        refs.push((step_name.clone(), v));
                    }
                }
                StepAction::MoveAction {
                    source,
                    destination,
                } => {
                    for v in extract_var_refs(source) {
                        refs.push((step_name.clone(), v));
                    }
                    for v in extract_var_refs(destination) {
                        refs.push((step_name.clone(), v));
                    }
                }
                StepAction::Copy {
                    source,
                    destination,
                    ..
                } => {
                    for v in extract_var_refs(source) {
                        refs.push((step_name.clone(), v));
                    }
                    for v in extract_var_refs(destination) {
                        refs.push((step_name.clone(), v));
                    }
                }
                StepAction::SetWorkingDir { path } => {
                    for v in extract_var_refs(path) {
                        refs.push((step_name.clone(), v));
                    }
                }
                StepAction::SetStopCommand { command } => {
                    for v in extract_var_refs(command) {
                        refs.push((step_name.clone(), v));
                    }
                }
                StepAction::SetEnv { variables } => {
                    for val in variables.values() {
                        for v in extract_var_refs(val) {
                            refs.push((step_name.clone(), v));
                        }
                    }
                }
                StepAction::WaitForOutput { pattern, .. } => {
                    for v in extract_var_refs(pattern) {
                        refs.push((step_name.clone(), v));
                    }
                }
                StepAction::DownloadGithubReleaseAsset {
                    tag_param,
                    asset_matcher,
                    destination,
                    filename,
                    ..
                } => {
                    // tag_param references a parameter name (not a ${var})
                    // but we check it anyway for completeness
                    for v in extract_var_refs(tag_param) {
                        refs.push((step_name.clone(), v));
                    }
                    for v in extract_var_refs(asset_matcher) {
                        refs.push((step_name.clone(), v));
                    }
                    for v in extract_var_refs(destination) {
                        refs.push((step_name.clone(), v));
                    }
                    if let Some(f) = filename {
                        for v in extract_var_refs(f) {
                            refs.push((step_name.clone(), v));
                        }
                    }
                }
                // SteamCMD steps may have extra_args with variable refs
                StepAction::SteamCmdInstall { extra_args, .. }
                | StepAction::SteamCmdUpdate { extra_args, .. } => {
                    for arg in extra_args {
                        for v in extract_var_refs(arg) {
                            refs.push((step_name.clone(), v));
                        }
                    }
                }
                // These don't contain variable references
                StepAction::Sleep { .. }
                | StepAction::SendSignal { .. }
                | StepAction::SetStopSignal { .. } => {}
            }
        }
        refs
    }

    /// Collect the names of variables produced by ResolveVariable steps.
    fn collect_resolved_vars(steps: &[PipelineStep]) -> Vec<String> {
        steps
            .iter()
            .filter_map(|s| match &s.action {
                StepAction::ResolveVariable { variable, .. } => Some(variable.clone()),
                _ => None,
            })
            .collect()
    }

    /// Built-in variables that are always available at pipeline execution
    /// time (injected by `build_variables`).
    fn builtin_var_names() -> Vec<&'static str> {
        vec!["server_dir", "server_id", "server_name"]
    }

    /// Check that every `${var}` in a step list is resolvable from the
    /// parameter set, built-in vars, or a prior ResolveVariable step.
    fn assert_vars_resolvable(
        template_name: &str,
        phase_name: &str,
        steps: &[PipelineStep],
        params: &[ConfigParameter],
    ) {
        let param_names: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
        let resolved_vars = collect_resolved_vars(steps);
        let builtins = builtin_var_names();
        let refs = collect_step_var_refs(steps);

        for (step_name, var) in &refs {
            let ok = param_names.contains(&var.as_str())
                || builtins.contains(&var.as_str())
                || resolved_vars.contains(var);
            assert!(
                ok,
                "Template '{}', phase '{}', step '{}': references undefined variable '${{{}}}'. \
                 Available params: {:?}, resolved: {:?}, builtins: {:?}",
                template_name, phase_name, step_name, var, param_names, resolved_vars, builtins,
            );
        }
    }

    #[test]
    fn install_steps_only_reference_defined_variables() {
        for t in list() {
            assert_vars_resolvable(
                &t.name,
                "install",
                &t.config.install_steps,
                &t.config.parameters,
            );
        }
    }

    #[test]
    fn update_steps_only_reference_defined_variables() {
        for t in list() {
            assert_vars_resolvable(
                &t.name,
                "update",
                &t.config.update_steps,
                &t.config.parameters,
            );
        }
    }

    #[test]
    fn start_steps_only_reference_defined_variables() {
        for t in list() {
            assert_vars_resolvable(
                &t.name,
                "start",
                &t.config.start_steps,
                &t.config.parameters,
            );
        }
    }

    #[test]
    fn stop_steps_only_reference_defined_variables() {
        for t in list() {
            assert_vars_resolvable(&t.name, "stop", &t.config.stop_steps, &t.config.parameters);
        }
    }

    #[test]
    fn args_only_reference_defined_variables() {
        for t in list() {
            let param_names: Vec<&str> = t
                .config
                .parameters
                .iter()
                .map(|p| p.name.as_str())
                .collect();
            let builtins = builtin_var_names();
            for arg in &t.config.args {
                for var in extract_var_refs(arg) {
                    let ok =
                        param_names.contains(&var.as_str()) || builtins.contains(&var.as_str());
                    assert!(
                        ok,
                        "Template '{}', args: references undefined variable '${{{}}}' in arg '{}'. \
                         Available: {:?}",
                        t.name, var, arg, param_names,
                    );
                }
            }
        }
    }

    // ── Parameter validation ─────────────────────────────────────────────

    #[test]
    fn parameter_names_are_valid_identifiers() {
        let ident_re = regex::Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*$").unwrap();
        for t in list() {
            for p in &t.config.parameters {
                assert!(
                    ident_re.is_match(&p.name),
                    "Template '{}': parameter name '{}' is not a valid identifier \
                     (must match [a-zA-Z_][a-zA-Z0-9_]*)",
                    t.name,
                    p.name,
                );
            }
        }
    }

    #[test]
    fn parameter_names_are_unique_within_template() {
        for t in list() {
            let mut names: Vec<&str> = t
                .config
                .parameters
                .iter()
                .map(|p| p.name.as_str())
                .collect();
            let original_len = names.len();
            names.sort();
            names.dedup();
            assert_eq!(
                names.len(),
                original_len,
                "Template '{}': duplicate parameter names detected",
                t.name,
            );
        }
    }

    #[test]
    fn select_parameters_have_options() {
        for t in list() {
            for p in &t.config.parameters {
                if matches!(p.param_type, ConfigParameterType::Select) {
                    assert!(
                        !p.options.is_empty(),
                        "Template '{}': select parameter '{}' has no options",
                        t.name,
                        p.name,
                    );
                }
            }
        }
    }

    #[test]
    fn select_parameter_defaults_are_in_options() {
        for t in list() {
            for p in &t.config.parameters {
                if matches!(p.param_type, ConfigParameterType::Select) {
                    if let Some(ref default) = p.default {
                        assert!(
                            p.options.contains(default),
                            "Template '{}': select parameter '{}' default '{}' is not in options {:?}",
                            t.name, p.name, default, p.options,
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn regex_patterns_are_valid() {
        for t in list() {
            for p in &t.config.parameters {
                if let Some(ref pattern) = p.regex {
                    let result = regex::Regex::new(pattern);
                    assert!(
                        result.is_ok(),
                        "Template '{}': parameter '{}' has invalid regex '{}': {}",
                        t.name,
                        p.name,
                        pattern,
                        result.unwrap_err(),
                    );
                }
            }
        }
    }

    #[test]
    fn regex_patterns_accept_default_values() {
        for t in list() {
            for p in &t.config.parameters {
                if let (Some(ref pattern), Some(ref default)) = (&p.regex, &p.default) {
                    let re = regex::Regex::new(pattern).unwrap();
                    assert!(
                        re.is_match(default),
                        "Template '{}': parameter '{}' default '{}' does not match its own regex '{}'",
                        t.name, p.name, default, pattern,
                    );
                }
            }
        }
    }

    #[test]
    fn at_most_one_version_parameter_per_template() {
        for t in list() {
            let version_count = t.config.parameters.iter().filter(|p| p.is_version).count();
            assert!(
                version_count <= 1,
                "Template '{}': has {} parameters with is_version=true, expected at most 1",
                t.name,
                version_count,
            );
        }
    }

    #[test]
    fn version_parameter_is_required() {
        for t in list() {
            for p in &t.config.parameters {
                if p.is_version {
                    assert!(
                        p.required,
                        "Template '{}': version parameter '{}' should be required",
                        t.name, p.name,
                    );
                }
            }
        }
    }

    // ── Step structure validation ────────────────────────────────────────

    #[test]
    fn step_names_are_non_empty() {
        for t in list() {
            let all_steps: Vec<(&str, &[PipelineStep])> = vec![
                ("install", &t.config.install_steps),
                ("update", &t.config.update_steps),
                ("start", &t.config.start_steps),
                ("stop", &t.config.stop_steps),
            ];
            for (phase, steps) in all_steps {
                for (i, step) in steps.iter().enumerate() {
                    assert!(
                        !step.name.trim().is_empty(),
                        "Template '{}', phase '{}': step {} has empty name",
                        t.name,
                        phase,
                        i,
                    );
                }
            }
        }
    }

    #[test]
    fn download_steps_have_non_empty_urls() {
        for t in list() {
            let all_steps: Vec<&[PipelineStep]> =
                vec![&t.config.install_steps, &t.config.update_steps];
            for steps in all_steps {
                for step in steps {
                    if let StepAction::Download { url, .. } = &step.action {
                        assert!(
                            !url.trim().is_empty(),
                            "Template '{}', step '{}': download URL is empty",
                            t.name,
                            step.name,
                        );
                        // URL should look like a URL (starts with http:// or https://)
                        // after stripping variable references
                        let stripped = url.replace("${", "").replace('}', "");
                        assert!(
                            stripped.starts_with("http://") || stripped.starts_with("https://"),
                            "Template '{}', step '{}': download URL '{}' doesn't look like an HTTP URL",
                            t.name, step.name, url,
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn resolve_variable_steps_produce_unique_names() {
        for t in list() {
            let all_steps: Vec<(&str, &[PipelineStep])> = vec![
                ("install", &t.config.install_steps),
                ("update", &t.config.update_steps),
            ];
            for (phase, steps) in all_steps {
                let vars = collect_resolved_vars(steps);
                let mut deduped = vars.clone();
                deduped.sort();
                deduped.dedup();
                assert_eq!(
                    vars.len(),
                    deduped.len(),
                    "Template '{}', phase '{}': duplicate ResolveVariable names detected: {:?}",
                    t.name,
                    phase,
                    vars,
                );
            }
        }
    }

    #[test]
    fn resolve_variable_steps_do_not_shadow_parameters() {
        for t in list() {
            let param_names: Vec<&str> = t
                .config
                .parameters
                .iter()
                .map(|p| p.name.as_str())
                .collect();
            let all_steps: Vec<(&str, &[PipelineStep])> = vec![
                ("install", &t.config.install_steps),
                ("update", &t.config.update_steps),
            ];
            for (phase, steps) in all_steps {
                let resolved = collect_resolved_vars(steps);
                for var in &resolved {
                    assert!(
                        !param_names.contains(&var.as_str()),
                        "Template '{}', phase '{}': ResolveVariable '{}' shadows a parameter name",
                        t.name,
                        phase,
                        var,
                    );
                }
            }
        }
    }

    // ── Update check validation ──────────────────────────────────────────

    #[test]
    fn update_check_api_urls_are_valid() {
        for t in list() {
            if let Some(ref uc) = t.config.update_check {
                if let crate::types::UpdateCheckProvider::Api { url, .. } = &uc.provider {
                    assert!(
                        url.starts_with("https://") || url.starts_with("http://"),
                        "Template '{}': update_check API URL '{}' is not a valid HTTP URL",
                        t.name,
                        url,
                    );
                }
            }
        }
    }

    #[test]
    fn templates_with_version_param_have_update_check() {
        for t in list() {
            let has_version_param = t.config.parameters.iter().any(|p| p.is_version);
            if has_version_param {
                assert!(
                    t.config.update_check.is_some(),
                    "Template '{}': has a version parameter but no update_check config",
                    t.name,
                );
            }
        }
    }

    #[test]
    fn templates_with_update_check_have_update_steps() {
        for t in list() {
            if t.config.update_check.is_some() {
                assert!(
                    !t.config.update_steps.is_empty(),
                    "Template '{}': has update_check config but no update_steps",
                    t.name,
                );
            }
        }
    }

    // ── Serialization round-trip ─────────────────────────────────────────

    #[test]
    fn template_config_survives_json_round_trip() {
        for t in list() {
            let json = serde_json::to_value(&t.config).unwrap_or_else(|e| {
                panic!(
                    "Template '{}': failed to serialize config to JSON: {}",
                    t.name, e
                )
            });
            let deserialized: crate::types::ServerConfig = serde_json::from_value(json.clone())
                .unwrap_or_else(|e| {
                    panic!(
                        "Template '{}': failed to deserialize config from JSON: {}\nJSON: {}",
                        t.name,
                        e,
                        serde_json::to_string_pretty(&json).unwrap()
                    )
                });

            // Verify key fields survive the round-trip
            assert_eq!(deserialized.name, t.config.name);
            assert_eq!(deserialized.binary, t.config.binary);
            assert_eq!(deserialized.args, t.config.args);
            assert_eq!(deserialized.parameters.len(), t.config.parameters.len());
            assert_eq!(
                deserialized.install_steps.len(),
                t.config.install_steps.len()
            );
            assert_eq!(deserialized.update_steps.len(), t.config.update_steps.len());
            assert_eq!(deserialized.stop_steps.len(), t.config.stop_steps.len());
            assert_eq!(deserialized.start_steps.len(), t.config.start_steps.len());

            // Verify parameter details survive
            for (orig, deser) in t.config.parameters.iter().zip(&deserialized.parameters) {
                assert_eq!(orig.name, deser.name, "param name mismatch in '{}'", t.name);
                assert_eq!(
                    orig.label, deser.label,
                    "param label mismatch in '{}'",
                    t.name
                );
                assert_eq!(
                    orig.required, deser.required,
                    "param required mismatch in '{}'",
                    t.name
                );
                assert_eq!(
                    orig.default, deser.default,
                    "param default mismatch in '{}'",
                    t.name
                );
                assert_eq!(
                    orig.is_version, deser.is_version,
                    "param is_version mismatch in '{}'",
                    t.name
                );
            }
        }
    }

    #[test]
    fn full_template_survives_json_round_trip() {
        for t in list() {
            let json = serde_json::to_value(t).unwrap_or_else(|e| {
                panic!("Template '{}': failed to serialize to JSON: {}", t.name, e)
            });
            let deserialized: ServerTemplate =
                serde_json::from_value(json.clone()).unwrap_or_else(|e| {
                    panic!(
                        "Template '{}': failed to deserialize from JSON: {}\nJSON: {}",
                        t.name,
                        e,
                        serde_json::to_string_pretty(&json).unwrap()
                    )
                });
            assert_eq!(deserialized.id, t.id);
            assert_eq!(deserialized.name, t.name);
            assert_eq!(deserialized.is_builtin, t.is_builtin);
            assert_eq!(deserialized.created_by, t.created_by);
        }
    }

    // ── Per-template specific checks ─────────────────────────────────────

    #[test]
    fn minecraft_paper_resolve_variable_precedes_download() {
        let mc = get(MINECRAFT_PAPER_UUID).unwrap();
        // The install pipeline must resolve paper_build BEFORE downloading
        let resolve_idx = mc
            .config
            .install_steps
            .iter()
            .position(|s| matches!(&s.action, StepAction::ResolveVariable { variable, .. } if variable == "paper_build"));
        let download_idx = mc
            .config
            .install_steps
            .iter()
            .position(|s| matches!(&s.action, StepAction::Download { url, .. } if url.contains("paper_build")));

        assert!(
            resolve_idx.is_some(),
            "Minecraft Paper install must have a ResolveVariable step for paper_build"
        );
        assert!(
            download_idx.is_some(),
            "Minecraft Paper install must have a Download step referencing paper_build"
        );
        assert!(
            resolve_idx.unwrap() < download_idx.unwrap(),
            "ResolveVariable for paper_build (step {}) must come before Download (step {})",
            resolve_idx.unwrap(),
            download_idx.unwrap(),
        );
    }

    #[test]
    fn minecraft_paper_update_steps_match_download_logic() {
        let mc = get(MINECRAFT_PAPER_UUID).unwrap();
        // update_steps should also resolve + download (same as install)
        let has_resolve = mc.config.update_steps.iter().any(|s| {
            matches!(&s.action, StepAction::ResolveVariable { variable, .. } if variable == "paper_build")
        });
        let has_download = mc.config.update_steps.iter().any(
            |s| matches!(&s.action, StepAction::Download { url, .. } if url.contains("paper")),
        );
        assert!(has_resolve, "update_steps must resolve paper_build");
        assert!(has_download, "update_steps must download paper jar");
    }

    #[test]
    fn minecraft_paper_server_properties_is_conditional() {
        let mc = get(MINECRAFT_PAPER_UUID).unwrap();
        let props_step = mc.config.install_steps.iter().find(|s| {
            matches!(&s.action, StepAction::WriteFile { path, .. } if path.contains("server.properties"))
        });
        assert!(
            props_step.is_some(),
            "Minecraft Paper should have a server.properties WriteFile step"
        );
        let step = props_step.unwrap();
        assert!(
            step.condition.is_some(),
            "server.properties step should be conditional (only write if not exists)"
        );
        let cond = step.condition.as_ref().unwrap();
        assert!(
            cond.path_not_exists.is_some(),
            "server.properties condition should use path_not_exists"
        );
    }

    #[test]
    fn minecraft_paper_stop_steps_send_stop_command() {
        let mc = get(MINECRAFT_PAPER_UUID).unwrap();
        let has_send_input =
            mc.config.stop_steps.iter().any(
                |s| matches!(&s.action, StepAction::SendInput { text } if text.contains("stop")),
            );
        assert!(
            has_send_input,
            "Minecraft Paper stop_steps must send 'stop' via SendInput"
        );
    }

    #[test]
    fn minecraft_paper_has_options_from_for_versions() {
        let mc = get(MINECRAFT_PAPER_UUID).unwrap();
        let version_param = mc.config.parameters.iter().find(|p| p.name == "mc_version");
        assert!(version_param.is_some(), "Should have mc_version parameter");
        let param = version_param.unwrap();
        assert!(
            param.options_from.is_some(),
            "mc_version should have options_from for dynamic version loading"
        );
        let opts = param.options_from.as_ref().unwrap();
        assert!(
            opts.url.contains("papermc.io"),
            "options_from URL should point to PaperMC API"
        );
    }

    #[test]
    fn valheim_uses_steamcmd_for_install() {
        let v = get(VALHEIM_UUID).unwrap();
        let has_steamcmd_step = v
            .config
            .install_steps
            .iter()
            .any(|s| matches!(&s.action, StepAction::SteamCmdInstall { .. }));
        assert!(
            has_steamcmd_step,
            "Valheim install_steps must use SteamCmdInstall"
        );
    }

    #[test]
    fn valheim_has_steamcmd_install_step() {
        let v = get(VALHEIM_UUID).unwrap();
        // The first install step should be a SteamCmdInstall action
        let first_step = &v.config.install_steps[0];
        assert!(
            matches!(&first_step.action, StepAction::SteamCmdInstall { app_id, anonymous, .. } if app_id.is_none() && *anonymous),
            "First Valheim install step should be a SteamCmdInstall with anonymous=true and app_id=None (uses config steam_app_id)"
        );
    }

    #[test]
    fn valheim_has_steam_app_id() {
        let v = get(VALHEIM_UUID).unwrap();
        assert_eq!(
            v.config.steam_app_id,
            Some(896660),
            "Valheim template should have steam_app_id=896660"
        );
    }

    #[test]
    fn valheim_requires_steamcmd_flag() {
        let v = get(VALHEIM_UUID).unwrap();
        assert!(
            v.requires_steamcmd,
            "Valheim template should have requires_steamcmd=true"
        );
    }

    #[test]
    fn valheim_update_uses_steamcmd_update() {
        let v = get(VALHEIM_UUID).unwrap();
        let has_update_step = v
            .config
            .update_steps
            .iter()
            .any(|s| matches!(&s.action, StepAction::SteamCmdUpdate { .. }));
        assert!(
            has_update_step,
            "Valheim update_steps must use SteamCmdUpdate"
        );
    }

    #[test]
    fn valheim_stop_uses_sigint() {
        let v = get(VALHEIM_UUID).unwrap();
        let has_sigint = v.config.stop_steps.iter().any(|s| {
            matches!(
                &s.action,
                StepAction::SendSignal {
                    signal: crate::types::StopSignal::Sigint
                }
            )
        });
        assert!(
            has_sigint,
            "Valheim stop_steps should send SIGINT for graceful shutdown"
        );
    }

    #[test]
    fn valheim_password_has_min_length_regex() {
        let v = get(VALHEIM_UUID).unwrap();
        let pw_param = v
            .config
            .parameters
            .iter()
            .find(|p| p.name == "password")
            .expect("Valheim should have a password parameter");
        assert!(
            pw_param.regex.is_some(),
            "Valheim password should have a regex for minimum length"
        );
        let re = regex::Regex::new(pw_param.regex.as_ref().unwrap()).unwrap();
        // Should reject short passwords
        assert!(!re.is_match("abcd"), "4-char password should be rejected");
        // Should accept 5+ char passwords
        assert!(re.is_match("abcde"), "5-char password should be accepted");
    }

    #[test]
    fn terraria_tshock_installs_via_download_and_extract() {
        let t = get(TERRARIA_TSHOCK_UUID).unwrap();
        let has_download = t.config.install_steps.iter().any(
            |s| matches!(&s.action, StepAction::DownloadGithubReleaseAsset { tag_param, .. } if tag_param == "tshock_version"),
        );
        let has_extract = t
            .config
            .install_steps
            .iter()
            .any(|s| matches!(&s.action, StepAction::Extract { .. }));
        let has_cleanup =
            t.config.install_steps.iter().any(
                |s| matches!(&s.action, StepAction::Delete { path, .. } if path.contains("zip")),
            );

        assert!(
            has_download,
            "TShock install should download the zip via GitHub release asset"
        );
        assert!(has_extract, "TShock install should extract the zip");
        assert!(has_cleanup, "TShock install should clean up the zip");
    }

    #[test]
    fn terraria_tshock_creates_worlds_directory() {
        let t = get(TERRARIA_TSHOCK_UUID).unwrap();
        let has_create_dir = t.config.install_steps.iter().any(
            |s| matches!(&s.action, StepAction::CreateDir { path } if path.contains("worlds")),
        );
        assert!(
            has_create_dir,
            "TShock install should create worlds directory"
        );
    }

    #[test]
    fn terraria_tshock_stop_sends_exit_command() {
        let t = get(TERRARIA_TSHOCK_UUID).unwrap();
        let has_exit =
            t.config.stop_steps.iter().any(
                |s| matches!(&s.action, StepAction::SendInput { text } if text.contains("exit")),
            );
        assert!(has_exit, "TShock stop_steps should send 'exit' command");
    }

    #[test]
    fn terraria_tshock_update_check_uses_github() {
        let t = get(TERRARIA_TSHOCK_UUID).unwrap();
        let uc = t
            .config
            .update_check
            .as_ref()
            .expect("TShock should have update_check");
        match &uc.provider {
            crate::types::UpdateCheckProvider::Api { url, .. } => {
                assert!(
                    url.contains("github.com") && url.contains("TShock"),
                    "TShock update_check should use GitHub TShock releases API, got: {}",
                    url
                );
            }
            other => {
                panic!(
                    "TShock update_check should use Api provider, got: {:?}",
                    other
                );
            }
        }
    }

    #[test]
    fn terraria_world_name_regex_rejects_special_chars() {
        let t = get(TERRARIA_TSHOCK_UUID).unwrap();
        let world_param = t
            .config
            .parameters
            .iter()
            .find(|p| p.name == "world_name")
            .expect("TShock should have world_name parameter");
        let re = regex::Regex::new(world_param.regex.as_ref().unwrap()).unwrap();
        assert!(re.is_match("my_world"), "underscores should be allowed");
        assert!(re.is_match("World-1"), "hyphens should be allowed");
        assert!(!re.is_match("my world"), "spaces should be rejected");
        assert!(
            !re.is_match("world;rm -rf /"),
            "semicolons should be rejected"
        );
        assert!(!re.is_match("../etc"), "path traversal should be rejected");
    }

    // ── Options-from validation ──────────────────────────────────────────

    #[test]
    fn options_from_urls_are_valid_http() {
        for t in list() {
            for p in &t.config.parameters {
                if let Some(ref opts) = p.options_from {
                    assert!(
                        opts.url.starts_with("https://") || opts.url.starts_with("http://"),
                        "Template '{}', parameter '{}': options_from URL '{}' is not HTTP(S)",
                        t.name,
                        p.name,
                        opts.url,
                    );
                }
            }
        }
    }

    // ── Isolation defaults ───────────────────────────────────────────────

    #[test]
    fn all_templates_have_default_isolation() {
        let default_iso = IsolationConfig::default();
        for t in list() {
            assert_eq!(
                t.config.isolation.enabled, default_iso.enabled,
                "Template '{}': isolation.enabled should use the default",
                t.name,
            );
        }
    }
}
