import { beforeEach, describe, expect, it } from "vitest";
import {
  createAccount,
  importAccount,
  listAccounts,
  unlockAccount,
  deleteAccount,
  NEX_SS58,
} from "@/wallet/desktopKeyring";
import { setSignerPair, getSignerAddress, clearSigner } from "@/chain/signer";

const LS_PREFIX = "nexchat-keystore:";

function installLocalStorage(): void {
  const store = new Map<string, string>();
  const ls = {
    getItem: (k: string) => store.get(k) ?? null,
    setItem: (k: string, v: string) => {
      store.set(k, v);
    },
    removeItem: (k: string) => {
      store.delete(k);
    },
    key: (i: number) => [...store.keys()][i] ?? null,
    get length() {
      return store.size;
    },
    clear: () => store.clear(),
  };
  Object.defineProperty(globalThis, "localStorage", { value: ls, configurable: true });
}

beforeEach(() => {
  installLocalStorage();
  clearSigner();
});

describe("desktopKeyring", () => {
  it("create → list → unlock → signRaw", async () => {
    const { mnemonic, address } = await createAccount("Alice", "pass1234");
    expect(mnemonic.split(" ").length).toBe(12);
    expect(address.length).toBeGreaterThan(10);

    const listed = await listAccounts();
    expect(listed).toHaveLength(1);
    expect(listed[0]!.name).toBe("Alice");

    const mockApi = async () => ({
      registry: {
        createType: () => ({
          sign: () => ({
            signature: "0x" + "ab".repeat(32),
          }),
        }),
      },
    });

    const { pair, signer } = await unlockAccount(address, "pass1234", mockApi);
    expect(pair.address).toBe(address);

    const raw = await signer.signRaw!({ address, data: "0x0102", type: "bytes" });
    expect(raw.signature).toMatch(/^0x/);

    setSignerPair(pair);
    expect(getSignerAddress()).toBe(address);
  });

  it("import rejects invalid mnemonic", async () => {
    await expect(importAccount("not a valid mnemonic", "x", "pw")).rejects.toThrow(
      /Invalid mnemonic/,
    );
  });

  it("stores encrypted keystore in localStorage", async () => {
    const { address } = await createAccount("Bob", "secret");
    const key = `${LS_PREFIX}${address}`;
    const raw = localStorage.getItem(key);
    expect(raw).toBeTruthy();
    const json = JSON.parse(raw!);
    expect(json.address).toBe(address);
    expect(json.encoding?.content).toBeTruthy();
    await deleteAccount(address);
    expect(localStorage.getItem(key)).toBeNull();
  });

  it("uses NEX SS58 prefix 273", () => {
    expect(NEX_SS58).toBe(273);
  });
});
