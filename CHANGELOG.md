# Changelog

All notable changes to AnyServer will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **GitHub Release Integration**
  - New `github_release_tag` parameter type for templates — allows dynamic version selection from GitHub repositories
  - New `download_github_release_asset` pipeline operation — downloads release assets with exact or regex pattern matching
  - GitHub API token configuration (Admin Panel → GitHub) for private repositories and higher rate limits
  - Searchable release selector UI with autocomplete and auto-selection of latest release

- **Process Sandbox & Isolation**
  - Per-server sandbox profiles with configurable Landlock, namespace isolation, and seccomp policies
  - Site-wide sandbox management feature flag (admin-controlled)
  - Host capability detection for available isolation features

- **Invite Code System**
  - Admin-managed invite codes for user registration
  - Pre-assigned roles, server permissions, and capabilities per invite
  - Configurable expiration and single-use enforcement

- **Per-Server Access Control**
  - Granular permission levels: Viewer, Operator, Manager, Admin, Owner
  - Server access manager UI for granting and revoking permissions
  - User search for permission assignment

- **Alert & Monitoring System**
  - SMTP email configuration with test email support
  - Configurable alert triggers with per-server mute controls
  - Cooldown-based alert dispatching

- **Pipeline System**
  - Multi-step install, update, and uninstall pipelines with real-time progress
  - Configurable stop steps with graceful shutdown sequences
  - Variable substitution and parameter overrides
  - Archive extraction (zip, tar, gzip, xz) with nested archive support

- **Update Checking**
  - Configurable update detection via API polling, template defaults, or shell commands
  - Cached results with configurable TTL
  - Bulk update status endpoint for dashboard overview

- **File Manager**
  - Browser-based file listing, reading, writing, and deletion
  - Directory creation and Unix permission management (chmod)
  - Path traversal protection

- **SFTP Server**
  - Built-in SFTP server for file transfers
  - Per-server SFTP credentials with constant-time authentication
  - Persistent host keys across restarts

- **System Health Dashboard**
  - Real-time CPU, memory, disk, and network metrics
  - Java and .NET runtime detection
  - SteamCMD availability checking
  - Database backup downloads

- **Template System**
  - Built-in templates for Minecraft (Paper), Valheim, and Terraria (TShock)
  - User-created templates with import/export support
  - Remote config import from URLs and GitHub folders

- **WebSocket Console**
  - Real-time server console output with log history replay
  - Global server status event stream
  - Automatic reconnection with ticket-based authentication

- **Security**
  - SSRF protection with DNS-rebinding-safe HTTP client
  - Rate limiting with per-tier configuration
  - Content Security Policy, security headers, and CORS configuration
  - JWT authentication with refresh token rotation and family-based revocation
  - API token support with scoped permissions

### Changed

- Terraria TShock template migrated to GitHub release integration

### Migration Guide

For templates using hardcoded GitHub download URLs, consider migrating to the
`github_release_tag` parameter type and `download_github_release_asset` pipeline
action for automatic version discovery.