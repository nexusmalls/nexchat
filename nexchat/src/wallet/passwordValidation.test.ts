import { describe, expect, it } from "vitest";
import { validateWalletPassword } from "@/wallet/passwordValidation";

describe("validateWalletPassword", () => {
  it("rejects short passwords", () => {
    expect(validateWalletPassword("abc").valid).toBe(false);
  });

  it("rejects single-category passwords", () => {
    expect(validateWalletPassword("abcdefgh").valid).toBe(false);
  });

  it("accepts mixed passwords", () => {
    expect(validateWalletPassword("pass1234").valid).toBe(true);
    expect(validateWalletPassword("Passw0rd").valid).toBe(true);
  });
});
