import { describe, expect, it } from "vitest";
import { parseQrRecipient } from "@/wallet/qrScanner";

describe("wallet/qrScanner", () => {
  it("parseQrRecipient accepts raw SS58", () => {
    const addr = "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY";
    expect(parseQrRecipient(addr)).toBe(addr);
  });

  it("parseQrRecipient reads substrate URI", () => {
    const addr = "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY";
    expect(parseQrRecipient(`substrate:${addr}`)).toBe(addr);
  });

  it("parseQrRecipient reads JSON address field", () => {
    const addr = "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY";
    expect(parseQrRecipient(JSON.stringify({ address: addr }))).toBe(addr);
  });

  it("parseQrRecipient rejects invalid text", () => {
    expect(parseQrRecipient("not-an-address")).toBeNull();
  });
});
