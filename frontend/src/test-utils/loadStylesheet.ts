import { readFileSync } from "fs";
import { resolve, dirname } from "path";

/**
 * Reads a CSS file and recursively resolves all `@import` directives,
 * returning the fully concatenated CSS as a single string.
 *
 * This is needed because the barrel `styles.css` uses `@import` to pull
 * in partials, but test helpers that read the file with `readFileSync`
 * only see the raw `@import` lines — not the actual CSS rules.
 */
export function loadStylesheetResolved(cssPath: string): string {
  const absolutePath = resolve(cssPath);
  return resolveImports(absolutePath, new Set());
}

function resolveImports(absolutePath: string, seen: Set<string>): string {
  if (seen.has(absolutePath)) return "";
  seen.add(absolutePath);

  const content = readFileSync(absolutePath, "utf-8");
  const dir = dirname(absolutePath);

  // Replace each @import "./path.css"; with the resolved content of that file.
  // Supports @import "path"; and @import url("path"); with single or double quotes.
  return content.replace(
    /@import\s+(?:url\()?["']([^"']+)["']\)?;?/g,
    (_match, importPath: string) => {
      const resolved = resolve(dir, importPath);
      return resolveImports(resolved, seen);
    },
  );
}

/**
 * Convenience: load the project's main `src/styles.css` with all imports
 * resolved. `srcDir` should be the `__dirname` of any file inside `src/`
 * (or a subdirectory — we walk up to find `styles.css`).
 */
export function loadProjectStylesheet(srcDir: string): string {
  // Walk up from srcDir until we find styles.css
  let dir = resolve(srcDir);
  for (let i = 0; i < 5; i++) {
    const candidate = resolve(dir, "styles.css");
    try {
      readFileSync(candidate);
      return loadStylesheetResolved(candidate);
    } catch {
      dir = dirname(dir);
    }
  }
  throw new Error(
    `Could not find styles.css starting from ${srcDir}`,
  );
}
