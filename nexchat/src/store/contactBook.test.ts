import { beforeEach, describe, expect, it } from "vitest";
import { Keyring } from "@polkadot/keyring";
import { cryptoWaitReady } from "@polkadot/util-crypto";
import {
  loadContacts,
  saveContacts,
  parseContactAddress,
  mergeRosters,
  loadUserRoster,
} from "@/store/contactBook";
import { rosterFromSeeds } from "@/p3/mentions";
import { NEX_SS58 } from "@/wallet/desktopKeyring";

const ACCOUNT = "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY";

beforeEach(() => {
  const store = new Map<string, string>();
  Object.defineProperty(globalThis, "localStorage", {
    value: {
      getItem: (k: string) => store.get(k) ?? null,
      setItem: (k: string, v: string) => {
        store.set(k, v);
      },
      removeItem: (k: string) => store.delete(k),
      key: () => null,
      get length() {
        return store.size;
      },
      clear: () => store.clear(),
    },
    configurable: true,
  });
  store.clear();
});

describe("contactBook", () => {
  it("parseContactAddress accepts SS58 42 and canonicalizes to 273", async () => {
    await cryptoWaitReady();
    const pair273 = new Keyring({ type: "sr25519", ss58Format: NEX_SS58 }).addFromUri("//Alice");
    const pair42 = new Keyring({ type: "sr25519", ss58Format: 42 }).addFromUri("//Alice");
    expect(parseContactAddress(pair42.address)).toBe(pair273.address);
  });

  it("save → load round-trip", () => {
    saveContacts(ACCOUNT, [{ address: "5xxx", label: "Bob", addedAt: 1 }]);
    expect(loadContacts(ACCOUNT)).toHaveLength(1);
    expect(loadUserRoster(ACCOUNT)[0]!.ref).toBe("Bob");
  });

  it("mergeRosters dedupes env and user by address", () => {
    const env = rosterFromSeeds(["//Alice", "//Bob"], ["addr-a", "addr-b"]);
    const user = [{ ref: "Carol", address: "addr-c", labels: ["Carol"] }];
    const merged = mergeRosters(env, user, "addr-a");
    expect(merged.map((m) => m.address)).toEqual(["addr-b", "addr-c"]);
  });
});
