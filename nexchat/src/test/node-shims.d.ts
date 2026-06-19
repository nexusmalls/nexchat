declare module "ws";

declare module "../../scripts/relay-token-verify.mjs" {
  export function verifyDeliveryToken(args: {
    ipkN: string;
    ipkE: string;
    s: string;
    p: string;
  }): Promise<boolean>;
}
