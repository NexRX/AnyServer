export {
  getToken,
  setToken,
  clearToken,
  isLoggedIn,
  ApiClientError,
  RateLimitError,
  rateLimitRetryAt,
  emitRateLimitEvent,
  initializeAuth,
} from "./core";
export {
  getAuthStatus,
  setup,
  login,
  register,
  logout,
  getMe,
  changePassword,
  logoutEverywhere,
  updateSettings,
  listSessions,
  revokeSession,
} from "./auth";

export {
  listUsers,
  getUser,
  updateUserRole,
  deleteUser,
  searchUsers,
} from "./admin";

export { importFromUrl, importFromFolder } from "./import";

export {
  listServers,
  getServer,
  createServer,
  updateServer,
  deleteServer,
  startServer,
  stopServer,
  cancelStop,
  restartServer,
  cancelRestart,
  sendCommand,
  sendSigint,
  resetServer,
  listDirectoryProcesses,
  killDirectoryProcesses,
  getServerStats,
  checkForUpdate,
  getUpdateStatus,
  markInstalled,
} from "./servers";

export {
  listPermissions,
  setPermission,
  removePermission,
} from "./permissions";

export {
  listFiles,
  readFile,
  writeFile,
  createDir,
  deletePath,
  getFilePermissions,
  chmodFile,
} from "./files";

export {
  installServer,
  updateServerPipeline,
  getPhaseStatus,
  cancelPhase,
  uninstallServer,
  killServer,
} from "./pipeline";

export {
  listTemplates,
  getTemplate,
  createTemplate,
  updateTemplate,
  deleteTemplate,
} from "./templates";

export {
  getSystemHealth,
  getJavaRuntimes,
  getDotnetRuntimes,
  getDotnetEnv,
  getSteamCmdStatus,
  validateSteamApp,
} from "./system";

export {
  getSmtpConfig,
  saveSmtpConfig,
  deleteSmtpConfig,
  sendTestEmail,
  getAlertConfig,
  saveAlertConfig,
  getServerAlerts,
  updateServerAlerts,
} from "./alerts";

export {
  createInviteCode,
  listInviteCodes,
  getInviteCode,
  updateInvitePermissions,
  deleteInviteCode,
  listUserPermissions,
  redeemInviteCode,
} from "./invite-codes";

export {
  getSandboxProfile,
  updateSandboxProfile,
  resetSandboxProfile,
  getSandboxCapabilities,
  toggleSandboxFeature,
} from "./sandbox";
