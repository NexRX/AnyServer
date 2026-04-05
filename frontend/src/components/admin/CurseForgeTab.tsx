import {
  type Component,
  createSignal,
  Show,
  onMount,
} from "solid-js";
import Loader from "../Loader";
import {
  getCurseForgeSettings,
  saveCurseForgeSettings,
} from "../../api/curseforge";

const CurseForgeTab: Component = () => {
  const [loading, setLoading] = createSignal(false);
  const [saving, setSaving] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [success, setSuccess] = createSignal<string | null>(null);
  const [hasKey, setHasKey] = createSignal(false);
  const [apiKey, setApiKey] = createSignal("");

  const loadSettings = async () => {
    setLoading(true);
    setError(null);
    try {
      const settings = await getCurseForgeSettings();
      setHasKey(settings.has_key);
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
      await saveCurseForgeSettings({
        api_key: apiKey().trim() || null,
      });
      setSuccess("CurseForge settings saved successfully");
      setApiKey("");
      await loadSettings();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  };

  const handleClear = async () => {
    if (!confirm("Are you sure you want to remove the CurseForge API key?")) {
      return;
    }

    setSaving(true);
    setError(null);
    setSuccess(null);

    try {
      await saveCurseForgeSettings({ api_key: null });
      setSuccess("CurseForge API key removed");
      setHasKey(false);
      setApiKey("");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div style={{ "max-width": "600px" }}>
      <h2>CurseForge Integration</h2>
      <p
        style={{
          color: "#9ca3af",
          "font-size": "0.9rem",
          "margin-bottom": "1.5rem",
        }}
      >
        Configure CurseForge API access for downloading modpack server packs. An
        API key is required to use the CurseForge integration in templates.
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

        <Show when={hasKey()}>
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
            ✓ CurseForge API key is configured
          </div>
        </Show>

        <form onSubmit={handleSave}>
          <div class="form-group" style={{ "margin-bottom": "1rem" }}>
            <label>
              CurseForge API Key
              <Show when={hasKey()}>
                {" "}
                <span style={{ color: "#9ca3af" }}>(optional — to update)</span>
              </Show>
            </label>
            <input
              type="password"
              value={apiKey()}
              onInput={(e) => setApiKey(e.currentTarget.value)}
              placeholder={
                hasKey()
                  ? "Enter new key to replace"
                  : "Enter CurseForge API key"
              }
              autocomplete="off"
            />
            <small>
              Generate an API key at{" "}
              <a
                href="https://console.curseforge.com/"
                target="_blank"
                rel="noopener noreferrer"
                style={{ color: "#60a5fa" }}
              >
                console.curseforge.com
              </a>
              . The key is required for all CurseForge template features
              (fetching file versions, downloading server packs).
            </small>
          </div>

          <div style={{ display: "flex", gap: "0.75rem" }}>
            <button
              type="submit"
              class="btn btn-primary"
              disabled={saving() || !apiKey().trim()}
            >
              {saving()
                ? "Saving..."
                : hasKey()
                  ? "Update Key"
                  : "Save Key"}
            </button>
            <Show when={hasKey()}>
              <button
                type="button"
                class="btn"
                onClick={handleClear}
                disabled={saving()}
              >
                Clear Key
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
            How It Works
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
              Template authors set a CurseForge project ID on a parameter to
              create a version selector.
            </li>
            <li>
              When users create a server, they see a dropdown of available file
              versions fetched from CurseForge.
            </li>
            <li>
              The pipeline automatically resolves and downloads the correct
              server pack — even when the listed file is a client pack.
            </li>
          </ul>
        </div>
      </Show>
    </div>
  );
};

export default CurseForgeTab;
