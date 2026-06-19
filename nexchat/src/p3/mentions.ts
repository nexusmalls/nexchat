// EN: @mention parsing and roster resolution (CHAT_P3 §4.2). Mentions travel inside the
// MLS envelope; the client builds a local「@me」index from decrypted `mentions`.
// CN: @提及解析与名册匹配（P3 §4.2）。提及在 MLS 信封内传递；客户端据解密后的 `mentions`
// 建立本地「@我」索引。

const TOKEN_RE = /@([A-Za-z0-9_-]+)/g;

export interface MentionMember {
  /** EN: canonical ref stored in envelope `mentions[]`. CN: 写入信封 `mentions[]` 的规范引用。 */
  ref: string;
  address: string;
  labels: string[];
}

/// EN: Extract `@token` labels from plain text (composer). CN: 从正文提取 `@token`。
export function parseMentionTokens(text: string): string[] {
  const out: string[] = [];
  for (const m of text.matchAll(TOKEN_RE)) {
    const t = m[1];
    if (t && !out.includes(t)) out.push(t);
  }
  return out;
}

/// EN: Map composer tokens to envelope member refs using the demo roster. CN: 用演示名册把输入 token 映射为信封成员引用。
export function resolveMentions(tokens: readonly string[], roster: readonly MentionMember[]): string[] {
  const refs: string[] = [];
  for (const t of tokens) {
    const hit = roster.find((m) =>
      m.labels.some((l) => l.toLowerCase() === t.toLowerCase()),
    );
    if (hit && !refs.includes(hit.ref)) refs.push(hit.ref);
  }
  return refs;
}

/// EN: Whether `refs` mentions `self`. CN: `refs` 是否提及 `self`。
export function isMentioned(refs: readonly string[], self: MentionMember): boolean {
  return refs.some(
    (r) =>
      r === self.ref ||
      self.labels.some((l) => l.toLowerCase() === r.toLowerCase()),
  );
}

/// EN: Build roster rows from dev seeds (`//Alice` → ref `Alice`). CN: 由 dev 种子构建名册行。
export function rosterFromSeeds(
  seeds: readonly string[],
  addresses: readonly string[],
): MentionMember[] {
  return seeds.map((seed, i) => {
    const name = seed.replace(/^\/\//, "");
    const addr = addresses[i] ?? "";
    return { ref: name, address: addr, labels: [name, name.toLowerCase(), addr] };
  });
}
