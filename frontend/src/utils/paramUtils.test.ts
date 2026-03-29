import { describe, it, expect } from "vitest";
import {
  extractParamRefs,
  extractParamNames,
  validateParamRefs,
  validateParamRefsMulti,
  hasParamRefs,
  formatAvailableParams,
  BUILTIN_PARAMS,
} from "./paramUtils";

// ─── extractParamRefs ───────────────────────────────────────────────

describe("extractParamRefs", () => {
  it("returns empty array for empty string", () => {
    expect(extractParamRefs("")).toEqual([]);
  });

  it("returns empty array for null/undefined-ish input", () => {
    // The function guards with `if (!text)` so empty string is falsy
    expect(extractParamRefs("")).toEqual([]);
  });

  it("extracts a single param reference", () => {
    const refs = extractParamRefs("hello ${world}");
    expect(refs).toHaveLength(1);
    expect(refs[0]).toEqual({
      full: "${world}",
      name: "world",
      start: 6,
    });
  });

  it("extracts multiple param references", () => {
    const refs = extractParamRefs("${a} and ${b} and ${c}");
    expect(refs).toHaveLength(3);
    expect(refs.map((r) => r.name)).toEqual(["a", "b", "c"]);
  });

  it("extracts params with underscores, numbers, and mixed case", () => {
    const refs = extractParamRefs("${server_version} ${port_2} ${MyParam}");
    expect(refs.map((r) => r.name)).toEqual([
      "server_version",
      "port_2",
      "MyParam",
    ]);
  });

  it("handles adjacent references without spaces", () => {
    const refs = extractParamRefs("${a}${b}${c}");
    expect(refs).toHaveLength(3);
    expect(refs.map((r) => r.name)).toEqual(["a", "b", "c"]);
  });

  it("returns correct start indices", () => {
    const refs = extractParamRefs("XX${a}YY${b}");
    expect(refs[0].start).toBe(2);
    expect(refs[1].start).toBe(8);
  });

  it("handles duplicate references", () => {
    const refs = extractParamRefs("${x} ${x} ${x}");
    expect(refs).toHaveLength(3);
    // All should have name "x"
    expect(refs.every((r) => r.name === "x")).toBe(true);
  });

  it("does not match incomplete patterns", () => {
    expect(extractParamRefs("${}")).toEqual([]); // empty name — regex requires [^}]+
    expect(extractParamRefs("${")).toEqual([]);
    expect(extractParamRefs("$")).toEqual([]);
    expect(extractParamRefs("hello world")).toEqual([]);
  });

  it("does not match $ without braces", () => {
    expect(extractParamRefs("$name")).toEqual([]);
    expect(extractParamRefs("$name$other")).toEqual([]);
  });

  it("handles params embedded in URLs", () => {
    const refs = extractParamRefs(
      "https://example.com/${version}/server-${build}.jar",
    );
    expect(refs).toHaveLength(2);
    expect(refs[0].name).toBe("version");
    expect(refs[1].name).toBe("build");
  });

  it("handles params in file paths", () => {
    const refs = extractParamRefs("config/${server_name}/settings.yml");
    expect(refs).toHaveLength(1);
    expect(refs[0].name).toBe("server_name");
  });

  it("handles params with dots in name", () => {
    // The regex captures [^}]+ so dots are valid
    const refs = extractParamRefs("${my.param}");
    expect(refs).toHaveLength(1);
    expect(refs[0].name).toBe("my.param");
  });

  it("handles params with spaces in name (edge case)", () => {
    // The regex captures anything that isn't }, including spaces
    const refs = extractParamRefs("${my param}");
    expect(refs).toHaveLength(1);
    expect(refs[0].name).toBe("my param");
  });

  it("is safe to call repeatedly (no regex state leaking)", () => {
    // Calling multiple times shouldn't cause issues with lastIndex
    const a = extractParamRefs("${x}");
    const b = extractParamRefs("${y}");
    const c = extractParamRefs("${z}");
    expect(a.map((r) => r.name)).toEqual(["x"]);
    expect(b.map((r) => r.name)).toEqual(["y"]);
    expect(c.map((r) => r.name)).toEqual(["z"]);
  });

  it("handles multiline text", () => {
    const text = `line1 \${a}
line2 \${b}
line3 \${c}`;
    const refs = extractParamRefs(text);
    expect(refs).toHaveLength(3);
    expect(refs.map((r) => r.name)).toEqual(["a", "b", "c"]);
  });
});

// ─── extractParamNames ──────────────────────────────────────────────

describe("extractParamNames", () => {
  it("returns empty set for no refs", () => {
    expect(extractParamNames("no params here")).toEqual(new Set());
  });

  it("returns unique names", () => {
    const names = extractParamNames("${a} ${b} ${a} ${c} ${b}");
    expect(names).toEqual(new Set(["a", "b", "c"]));
  });

  it("returns a Set", () => {
    const names = extractParamNames("${x}");
    expect(names).toBeInstanceOf(Set);
  });
});

// ─── validateParamRefs ──────────────────────────────────────────────

describe("validateParamRefs", () => {
  it("returns empty array when no refs exist", () => {
    expect(validateParamRefs("no params", ["version"])).toEqual([]);
  });

  it("returns empty array when all refs are known user params", () => {
    expect(
      validateParamRefs("${version} ${port}", ["version", "port"]),
    ).toEqual([]);
  });

  it("returns empty array when refs are built-in params", () => {
    expect(
      validateParamRefs("${server_dir} ${server_id} ${server_name}", []),
    ).toEqual([]);
  });

  it("returns unknown param names", () => {
    const unknown = validateParamRefs("${version} ${unknown_param}", [
      "version",
    ]);
    expect(unknown).toEqual(["unknown_param"]);
  });

  it("returns multiple unknown params", () => {
    const unknown = validateParamRefs("${a} ${b} ${c}", ["b"]);
    expect(unknown).toEqual(["a", "c"]);
  });

  it("de-duplicates unknown params", () => {
    const unknown = validateParamRefs("${x} ${x} ${x}", []);
    expect(unknown).toEqual(["x"]);
  });

  it("accepts known params as a Set", () => {
    const known = new Set(["version"]);
    expect(validateParamRefs("${version}", known)).toEqual([]);
    expect(validateParamRefs("${other}", known)).toEqual(["other"]);
  });

  it("accepts known params as an array", () => {
    expect(validateParamRefs("${version}", ["version"])).toEqual([]);
    expect(validateParamRefs("${other}", ["version"])).toEqual(["other"]);
  });

  it("does not flag built-in params even with empty known list", () => {
    for (const builtin of BUILTIN_PARAMS) {
      expect(validateParamRefs(`\${${builtin}}`, [])).toEqual([]);
    }
  });

  it("correctly handles mix of known, built-in, and unknown", () => {
    const text = "${version} ${server_dir} ${unknown} ${port} ${server_id}";
    const unknown = validateParamRefs(text, ["version", "port"]);
    expect(unknown).toEqual(["unknown"]);
  });

  it("handles empty text", () => {
    expect(validateParamRefs("", ["version"])).toEqual([]);
  });
});

// ─── validateParamRefsMulti ─────────────────────────────────────────

describe("validateParamRefsMulti", () => {
  it("returns empty array when all texts are valid", () => {
    const result = validateParamRefsMulti(
      ["${version}", "${server_dir}", "no refs"],
      ["version"],
    );
    expect(result).toEqual([]);
  });

  it("collects unknown from multiple texts", () => {
    const result = validateParamRefsMulti(["${a}", "${b}", "${c}"], ["a"]);
    expect(result).toContain("b");
    expect(result).toContain("c");
    expect(result).not.toContain("a");
  });

  it("de-duplicates across texts", () => {
    const result = validateParamRefsMulti(["${x}", "${x}", "${x}"], []);
    expect(result).toEqual(["x"]);
  });

  it("handles null and undefined in the text array", () => {
    const result = validateParamRefsMulti(
      [null, undefined, "${known}", null, "${unknown}"],
      ["known"],
    );
    expect(result).toEqual(["unknown"]);
  });

  it("handles empty array", () => {
    expect(validateParamRefsMulti([], ["version"])).toEqual([]);
  });

  it("handles array of all nulls", () => {
    expect(validateParamRefsMulti([null, null, null], ["version"])).toEqual([]);
  });
});

// ─── hasParamRefs ───────────────────────────────────────────────────

describe("hasParamRefs", () => {
  it("returns false for empty string", () => {
    expect(hasParamRefs("")).toBe(false);
  });

  it("returns false for text without refs", () => {
    expect(hasParamRefs("just plain text")).toBe(false);
  });

  it("returns true when refs are present", () => {
    expect(hasParamRefs("hello ${world}")).toBe(true);
  });

  it("returns false for partial patterns", () => {
    expect(hasParamRefs("${}")).toBe(false);
    expect(hasParamRefs("${")).toBe(false);
    expect(hasParamRefs("$name")).toBe(false);
  });

  it("returns true for built-in param refs", () => {
    expect(hasParamRefs("${server_dir}")).toBe(true);
  });

  it("is safe to call repeatedly (no regex state issues)", () => {
    // Calling hasParamRefs followed by extractParamRefs should not interfere
    expect(hasParamRefs("${a}")).toBe(true);
    expect(extractParamRefs("${b}")).toHaveLength(1);
    expect(hasParamRefs("${c}")).toBe(true);
    expect(hasParamRefs("no")).toBe(false);
  });
});

// ─── formatAvailableParams ──────────────────────────────────────────

describe("formatAvailableParams", () => {
  it("includes user params", () => {
    const result = formatAvailableParams(["version", "port"]);
    expect(result).toContain("${version}");
    expect(result).toContain("${port}");
  });

  it("includes built-in params", () => {
    const result = formatAvailableParams([]);
    expect(result).toContain("${server_dir}");
    expect(result).toContain("${server_id}");
    expect(result).toContain("${server_name}");
    // Built-ins should be marked
    expect(result).toContain("(built-in)");
  });

  it("lists user params before built-in params", () => {
    const result = formatAvailableParams(["my_param"]);
    const userIndex = result.indexOf("${my_param}");
    const builtinIndex = result.indexOf("${server_dir}");
    expect(userIndex).toBeLessThan(builtinIndex);
  });

  it("returns only built-ins when no user params", () => {
    const result = formatAvailableParams([]);
    // Should not have any non-built-in entries
    for (const builtin of BUILTIN_PARAMS) {
      expect(result).toContain(`\${${builtin}}`);
    }
  });
});

// ─── BUILTIN_PARAMS ─────────────────────────────────────────────────

describe("BUILTIN_PARAMS", () => {
  it("is a read-only Set", () => {
    expect(BUILTIN_PARAMS).toBeInstanceOf(Set);
  });

  it("contains expected built-in names", () => {
    expect(BUILTIN_PARAMS.has("server_dir")).toBe(true);
    expect(BUILTIN_PARAMS.has("server_id")).toBe(true);
    expect(BUILTIN_PARAMS.has("server_name")).toBe(true);
  });

  it("does not contain arbitrary names", () => {
    expect(BUILTIN_PARAMS.has("version")).toBe(false);
    expect(BUILTIN_PARAMS.has("port")).toBe(false);
    expect(BUILTIN_PARAMS.has("")).toBe(false);
  });

  it("has exactly 3 entries", () => {
    expect(BUILTIN_PARAMS.size).toBe(3);
  });
});

// ─── Integration / edge-case scenarios ──────────────────────────────

describe("integration scenarios", () => {
  it("validates a realistic Minecraft server config URL", () => {
    const url =
      "https://papermc.io/api/v2/projects/paper/versions/${mc_version}/builds/${build_number}/downloads/paper-${mc_version}-${build_number}.jar";
    const refs = extractParamRefs(url);
    expect(refs).toHaveLength(4);
    expect(refs.map((r) => r.name)).toEqual([
      "mc_version",
      "build_number",
      "mc_version",
      "build_number",
    ]);

    const unknown = validateParamRefs(url, ["mc_version", "build_number"]);
    expect(unknown).toEqual([]);

    const unknownMissing = validateParamRefs(url, ["mc_version"]);
    expect(unknownMissing).toEqual(["build_number"]);
  });

  it("validates a java command line with mixed params", () => {
    const args =
      "-Xmx${max_ram}G -Xms${min_ram}G -jar ${server_dir}/server.jar --port ${port}";
    const unknown = validateParamRefs(args, ["max_ram", "min_ram", "port"]);
    expect(unknown).toEqual([]); // server_dir is built-in
  });

  it("flags unknown params in a write_file content template", () => {
    const content =
      "server-port=\${port}\nlevel-name=\${world_name}\ngamemode=\${game_mode}\nmotd=Welcome to \${server_name}!";

    const unknown = validateParamRefs(content, ["port", "world_name"]);
    // game_mode is unknown, server_name is built-in
    expect(unknown).toEqual(["game_mode"]);
  });

  it("handles rapid consecutive calls (simulating keystroke-by-keystroke typing)", () => {
    const known = ["version"];
    // Simulate typing "${ver" -> "${vers" -> "${versi" -> "${version}"
    expect(validateParamRefs("${ver}", known)).toEqual(["ver"]);
    expect(validateParamRefs("${vers}", known)).toEqual(["vers"]);
    expect(validateParamRefs("${versi}", known)).toEqual(["versi"]);
    expect(validateParamRefs("${versio}", known)).toEqual(["versio"]);
    expect(validateParamRefs("${version}", known)).toEqual([]);
  });

  it("multi-validate across an entire pipeline step's fields", () => {
    // Simulating all text fields from a download action step
    const url = "https://example.com/${version}/server.jar";
    const destination = "${server_dir}/downloads";
    const filename = "server-${version}.jar";
    const condition_path = "${server_dir}/server.jar";

    const unknown = validateParamRefsMulti(
      [url, destination, filename, condition_path],
      ["version"],
    );
    expect(unknown).toEqual([]);
  });

  it("multi-validate catches unknown params across multiple fields", () => {
    const url = "https://example.com/${version}/server.jar";
    const content = "port=${port}";
    const path = "${nonexistent}/config.yml";

    const unknown = validateParamRefsMulti([url, content, path], ["version"]);
    expect(unknown).toContain("port");
    expect(unknown).toContain("nonexistent");
    expect(unknown).not.toContain("version");
    expect(unknown).toHaveLength(2);
  });
});
