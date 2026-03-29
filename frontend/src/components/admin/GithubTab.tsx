import {
  type Component,
  createSignal,
  Show,
  onMount,
} from "solid-js";
import Loader from "../Loader";
import { getGithubSettings, saveGithubSettings } from "../../api/github";

const GithubTab: Component = () => {
  const [loading, setLoading] = createSignal(false);
  const [saving, setSaving] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [success, setSuccess] = createSignal<string | null>(null);
  const [hasToken, setHasToken] = createSignal(false);
  const [apiToken, setApiToken] = createSignal("");

  const loadSettings = async () => {
    setLoading(true);
    setError(null);
    try {
      const settings = await getGithubSettings();
      setHasToken(settings.has_token);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  };

  onMount(() => {
    loadSettings();
  });

  const handleSave = async (e: Event) => {
    e.preventDefault();
    setSaving(true);
    setError(null);
    setSuccess(null);

    try {
      await saveGithubSettings({
        api_token: apiToken().trim() || null,
      });
      setSuccess("GitHub settings saved successfully");
      setApiToken("");
      await loadSettings();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  };

  const handleClear = async () => {
    if (!confirm("Are you sure you want to remove the GitHub API token?")) {
      return;
    }

    setSaving(true);
    setError(null);
    setSuccess(null);

    try {
      await saveGithubSettings({ api_token: null });
      setSuccess("GitHub API token removed");
      setHasToken(false);
      setApiToken("");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div style={{ "max-width": "600px" }}>
      <h2>GitHub Integration</h2>
      <p
        style={{
          color: "#9ca3af",
          "font-size": "0.9rem",
          "margin-bottom": "1.5rem",
        }}
      >
        Configure GitHub API access for fetching release information. A token is
        optional for public repositories but required for private repositories
        and provides higher rate limits.
      </p>

      <Show when={loading()}>
        <Loader />
      </Show>

      <Show when={!loading()}>
        <Show when={error()}>
          <div
            style={{
              background: "#7f1d1d",
              border: "1px solid #991b1b",
              "border-radius": "0.375rem",
              padding: "0.75rem",
              color: "#fca5a5",
              "margin-bottom": "1rem",
              "font-size": "0.9rem",
            }}
          >
            {error()}
          </div>
        </Show>

        <Show when={success()}>
          <div
            style={{
              background: "#064e3b",
              border: "1px solid #047857",
              "border-radius": "0.375rem",
              padding: "0.75rem",
              color: "#6ee7b7",
              "margin-bottom": "1rem",
              "font-size": "0.9rem",
            }}
          >
            {success()}
          </div>
        </Show>

        <Show when={hasToken()}>
          <div
            style={{
              background: "#064e3b",
              border: "1px solid #047857",
              "border-radius": "0.375rem",
              padding: "0.75rem",
              color: "#6ee7b7",
              "margin-bottom": "1.5rem",
              "font-size": "0.9rem",
            }}
          >
            ✓ GitHub API token is configured
          </div>
        </Show>

        <form onSubmit={handleSave}>
          <div class="form-group" style={{ "margin-bottom": "1rem" }}>
            <label>
              GitHub Personal Access Token
              <Show when={hasToken()}>
                {" "}
                <span style={{ color: "#9ca3af" }}>(optional - to update)</span>
              </Show>
            </label>
            <input
              type="password"
              value={apiToken()}
              onInput={(e) => setApiToken(e.currentTarget.value)}
              placeholder={
                hasToken() ? "Enter new token to replace" : "Enter GitHub token"
              }
              autocomplete="off"
            />
            <small>
              Create a token at{" "}
              <a
                href="https://github.com/settings/tokens"
                target="_blank"
                rel="noopener noreferrer"
                style={{ color: "#60a5fa" }}
              >
                github.com/settings/tokens
              </a>
              . No special scopes are required for public repositories. For
              private repositories, grant the <code>repo</code> scope.
            </small>
          </div>

          <div style={{ display: "flex", gap: "0.75rem" }}>
            <button
              type="submit"
              class="btn btn-primary"
              disabled={saving() || !apiToken().trim()}
            >
              {saving()
                ? "Saving..."
                : hasToken()
                  ? "Update Token"
                  : "Save Token"}
            </button>
            <Show when={hasToken()}>
              <button
                type="button"
                class="btn"
                onClick={handleClear}
                disabled={saving()}
              >
                Clear Token
              </button>
            </Show>
          </div>
        </form>

        <div
          style={{
            "margin-top": "2rem",
            padding: "1rem",
            background: "#1f2937",
            "border-radius": "0.375rem",
            border: "1px solid #374151",
          }}
        >
          <h3 style={{ "margin-bottom": "0.5rem", "font-size": "1rem" }}>
            Rate Limits
          </h3>
          <ul
            style={{
              "list-style": "disc",
              padding: "0 0 0 1.5rem",
              "font-size": "0.9rem",
              color: "#9ca3af",
            }}
          >
            <li>
              Without token: 60 requests/hour per IP (unauthenticated API)
            </li>
            <li>With token: 5,000 requests/hour (authenticated API)</li>
          </ul>
        </div>
      </Show>
    </div>
  );
};

export default GithubTab;
