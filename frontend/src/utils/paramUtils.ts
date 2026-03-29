const PARAM_REF_RE = /\$\{([^}]+)\}/g;

export const BUILTIN_PARAMS: ReadonlySet<string> = new Set([
  "server_dir",
  "server_id",
  "server_name",
]);

export interface ParamRef {
  full: string;
  name: string;
  start: number;
}

export function extractParamRefs(text: string): ParamRef[] {
  if (!text) return [];
  const refs: ParamRef[] = [];
  PARAM_REF_RE.lastIndex = 0;
  let match: RegExpExecArray | null;
  while ((match = PARAM_REF_RE.exec(text)) !== null) {
    refs.push({
      full: match[0],
      name: match[1],
      start: match.index,
    });
  }
  return refs;
}

export function extractParamNames(text: string): Set<string> {
  return new Set(extractParamRefs(text).map((r) => r.name));
}

export function validateParamRefs(
  text: string,
  knownUserParams: ReadonlySet<string> | string[],
): string[] {
  const known =
    knownUserParams instanceof Set ? knownUserParams : new Set(knownUserParams);

  const refs = extractParamRefs(text);
  const unknown: string[] = [];
  const seen = new Set<string>();

  for (const ref of refs) {
    if (seen.has(ref.name)) continue;
    seen.add(ref.name);

    if (!BUILTIN_PARAMS.has(ref.name) && !known.has(ref.name)) {
      unknown.push(ref.name);
    }
  }

  return unknown;
}

export function validateParamRefsMulti(
  texts: (string | null | undefined)[],
  knownUserParams: ReadonlySet<string> | string[],
): string[] {
  const all = new Set<string>();
  for (const text of texts) {
    if (text) {
      for (const name of validateParamRefs(text, knownUserParams)) {
        all.add(name);
      }
    }
  }
  return [...all];
}

export function hasParamRefs(text: string): boolean {
  if (!text) return false;
  PARAM_REF_RE.lastIndex = 0;
  return PARAM_REF_RE.test(text);
}

export function formatAvailableParams(userParamNames: string[]): string {
  const parts: string[] = [];
  for (const name of userParamNames) {
    parts.push(`\${${name}}`);
  }
  for (const name of BUILTIN_PARAMS) {
    parts.push(`\${${name}} (built-in)`);
  }
  return parts.join(", ");
}
