export function uuidV7(nowMs = Date.now()): string {
  if (!Number.isSafeInteger(nowMs) || nowMs < 0) {
    throw new Error("uuidV7 requires a non-negative integer timestamp");
  }

  const bytes = crypto.getRandomValues(new Uint8Array(16));
  let timestamp = BigInt(nowMs);

  for (let index = 5; index >= 0; index -= 1) {
    bytes[index] = Number(timestamp & 0xffn);
    timestamp >>= 8n;
  }

  bytes[6] = (bytes[6] & 0x0f) | 0x70;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;

  const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0"));
  return [
    hex.slice(0, 4).join(""),
    hex.slice(4, 6).join(""),
    hex.slice(6, 8).join(""),
    hex.slice(8, 10).join(""),
    hex.slice(10).join(""),
  ].join("-");
}
