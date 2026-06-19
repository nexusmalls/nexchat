// EN: Decoupling invariant guard (design §2/§3, milestone M3). The decentralized 1:1 stack
// (`@/crypto-dr/*`) and the OpenMLS group stack (`@/mls/*`) are independent modules that MUST
// NOT import each other at compile time — neither engine, state, nor helpers cross the line
// (shared conventions are inlined or live in neutral modules like `@/wallet/*`, `@/relay/*`,
// `@/util/*`). This test scans real `import ... from "..."` / `import("...")` statements
// (comments mentioning the path use backticks, not quotes, so they are not matched) in both
// directories and fails loudly if the boundary is ever breached.
// CN: 解耦不变量守卫（设计 §2/§3，里程碑 M3）。去中心化 1:1 栈（`@/crypto-dr/*`）与 OpenMLS 群栈
// （`@/mls/*`）是相互独立的模块，编译期**不得**互相 import——引擎、状态、helper 一律不得越界
// （共享约定就地内联，或放在中立模块如 `@/wallet/*`、`@/relay/*`、`@/util/*`）。本测试扫描两侧目录
// 中真实的 `import ... from "..."` / `import("...")` 语句（注释里用反引号提及路径不会被匹配），
// 一旦越界即显式失败。

import { readdirSync, readFileSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const DR_DIR = fileURLToPath(new URL(".", import.meta.url));
const MLS_DIR = fileURLToPath(new URL("../mls", import.meta.url));

/// Recursively list `.ts` source files (excluding test files).
function listSources(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir)) {
    const full = `${dir}/${entry}`;
    if (statSync(full).isDirectory()) {
      out.push(...listSources(full));
    } else if (entry.endsWith(".ts") && !entry.endsWith(".test.ts")) {
      out.push(full);
    }
  }
  return out;
}

/// Match a forbidden module prefix only inside an actual import (quoted) specifier.
function importsForbidden(source: string, forbiddenPrefix: string): boolean {
  const re = new RegExp(
    `(?:from|import)\\s*\\(?\\s*["']${forbiddenPrefix.replace(/[/]/g, "\\/")}`,
  );
  return source
    .split("\n")
    .some((line) => !line.trimStart().startsWith("//") && re.test(line));
}

describe("DR ↔ MLS decoupling invariant (M3)", () => {
  it("no @/crypto-dr/* source imports @/mls/* (or ../mls)", () => {
    const offenders = listSources(DR_DIR).filter(
      (f) => importsForbidden(readFileSync(f, "utf8"), "@/mls/") ||
        importsForbidden(readFileSync(f, "utf8"), "../mls"),
    );
    expect(offenders.map((f) => f.split("/crypto-dr/")[1])).toEqual([]);
  });

  it("no @/mls/* source imports @/crypto-dr/* (or ../crypto-dr)", () => {
    const offenders = listSources(MLS_DIR).filter(
      (f) => importsForbidden(readFileSync(f, "utf8"), "@/crypto-dr/") ||
        importsForbidden(readFileSync(f, "utf8"), "../crypto-dr"),
    );
    expect(offenders.map((f) => f.split("/mls/")[1])).toEqual([]);
  });
});
