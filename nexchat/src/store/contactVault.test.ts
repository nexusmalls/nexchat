import { describe, it, expect, beforeEach } from "vitest";
import { keyVault } from "@/keyvault/keyvault";
import {
  buildVaultFromLocal,
  decryptVaultBlob,
  encryptVaultBlob,
  mergeVaultBlobs,
  tombstonesForRemoved,
  vaultToSavedContacts,
  type ContactVaultBlob,
} from "@/store/contactVault";

describe("contactVault", () => {
  beforeEach(() => {
    keyVault.initForTest("5AliceTestAccount");
  });

  it("mergeVaultBlobs keeps newer label per address", () => {
    const a: ContactVaultBlob = {
      v: 1,
      updated_at: 100,
      device_id: "a",
      contacts: [
        {
          address: "5Bob",
          label: "Bob",
          addedAt: 50,
          updated_at: 100,
        },
      ],
    };
    const b: ContactVaultBlob = {
      v: 1,
      updated_at: 200,
      device_id: "b",
      contacts: [
        {
          address: "5Bob",
          label: "Robert",
          addedAt: 50,
          updated_at: 200,
        },
      ],
    };
    const m = mergeVaultBlobs(a, b);
    expect(m.contacts).toHaveLength(1);
    expect(m.contacts[0]!.label).toBe("Robert");
    expect(m.updated_at).toBe(200);
  });

  it("tombstonesForRemoved marks deleted contacts", () => {
    const last: ContactVaultBlob = {
      v: 1,
      updated_at: 1,
      device_id: "x",
      contacts: [
        { address: "5Bob", label: "Bob", addedAt: 1, updated_at: 1 },
        { address: "5Carol", label: "Carol", addedAt: 2, updated_at: 2 },
      ],
    };
    const local = [{ address: "5Bob", label: "Bob", addedAt: 1 }];
    const tombs = tombstonesForRemoved(last, local, 99);
    expect(tombs).toHaveLength(1);
    expect(tombs[0]!.address).toBe("5Carol");
    expect(tombs[0]!.tombstone).toBe(true);
    const merged = mergeVaultBlobs(buildVaultFromLocal(local, "d", 50), {
      v: 1,
      updated_at: 99,
      device_id: "d",
      contacts: tombs,
    });
    expect(vaultToSavedContacts(merged).map((c) => c.address)).toEqual(["5Bob"]);
  });

  it("encrypt/decrypt round-trip", async () => {
    const blob = buildVaultFromLocal(
      [{ address: "5Carol", label: "Carol", addedAt: 10, updatedAt: 10 }],
      "dev-1",
      42,
    );
    const packed = await encryptVaultBlob(blob);
    const back = await decryptVaultBlob(packed);
    expect(back.contacts[0]!.address).toBe("5Carol");
    expect(back.contacts[0]!.label).toBe("Carol");
  });
});
