export {};

type EncodeRequest = { bitmap: ImageBitmap };
type EncodeResponse = { png?: string; error?: string };

const scope = globalThis as unknown as {
  onmessage: ((event: MessageEvent<EncodeRequest>) => void) | null;
  postMessage(message: EncodeResponse): void;
};

scope.onmessage = (event) => {
  const bitmap = event.data?.bitmap;
  if (!bitmap) {
    scope.postMessage({ error: "capture_failed" });
    return;
  }
  const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
  const ctx = canvas.getContext("2d");
  if (!ctx) {
    bitmap.close();
    scope.postMessage({ error: "capture_failed" });
    return;
  }
  ctx.drawImage(bitmap, 0, 0);
  bitmap.close();
  void canvas.convertToBlob({ type: "image/png" }).then(
    async (blob) => {
      const bytes = new Uint8Array(await blob.arrayBuffer());
      scope.postMessage({ png: bytesToBase64(bytes) });
    },
    () => scope.postMessage({ error: "capture_failed" }),
  );
};

function bytesToBase64(bytes: Uint8Array) {
  const encode = (bytes as Uint8Array & { toBase64?: () => string }).toBase64;
  if (encode) return encode.call(bytes);
  let binary = "";
  const chunk = 8192;
  for (let index = 0; index < bytes.length; index += chunk) {
    const slice = bytes.subarray(index, index + chunk);
    let part = "";
    for (let offset = 0; offset < slice.length; offset += 1) part += String.fromCharCode(slice[offset]);
    binary += part;
  }
  return btoa(binary);
}
