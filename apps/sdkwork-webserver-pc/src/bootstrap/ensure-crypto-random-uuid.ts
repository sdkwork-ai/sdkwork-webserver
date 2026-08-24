/**
 * Patch `crypto.randomUUID` on HTTP / legacy hosts before admin create flows run.
 * Idempotent: keeps a working native implementation when present.
 */
export function ensureCryptoRandomUuid(): void {
  const crypto = globalThis.crypto;
  if (!crypto?.getRandomValues) {
    return;
  }

  const native = crypto.randomUUID;
  if (typeof native === "function") {
    try {
      native.call(crypto);
      return;
    } catch {
      // Non-secure contexts may expose randomUUID but reject calls.
    }
  }

  Object.defineProperty(crypto, "randomUUID", {
    configurable: true,
    writable: true,
    value: function randomUUID(this: Crypto): `${string}-${string}-${string}-${string}-${string}` {
      const bytes = new Uint8Array(16);
      this.getRandomValues(bytes);
      bytes[6] = ((bytes[6] ?? 0) & 0x0f) | 0x40;
      bytes[8] = ((bytes[8] ?? 0) & 0x3f) | 0x80;
      const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
      return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
    },
  });
}
