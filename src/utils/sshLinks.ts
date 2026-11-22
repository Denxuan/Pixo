const HTTP = /https?:\/\/[^\s<>"'`]+/gi;
const TOKEN = /[^\s'"<>|]+/g;

export type HttpRange = { start: number; end: number; text: string };

export function httpRanges(line: string): HttpRange[] {
  const found: HttpRange[] = [];
  for (const match of line.matchAll(HTTP)) {
    const raw = match[0];
    const text = raw.replace(/[),.;:!?]+$/g, "");
    if (!/^https?:\/\/\S+$/i.test(text)) continue;
    const start = match.index ?? 0;
    found.push({ start, end: start + text.length, text });
  }
  return found;
}

export function pathAt(line: string, index: number): string | null {
  if (index < 0) return null;
  for (const match of line.matchAll(TOKEN)) {
    const start = match.index ?? 0;
    const value = match[0];
    if (index < start || index >= start + value.length) continue;
    const cleaned = value.replace(/[),.;:]+$/g, "");
    if (cleaned.startsWith("/") || cleaned.startsWith("~/")) return cleaned;
    return null;
  }
  return null;
}

export function pathAtColumn(cells: string[], column: number): string | null {
  if (column < 0 || column >= cells.length) return null;
  let text = "";
  const indexAt: number[] = [];
  for (const cell of cells) {
    indexAt.push(text.length);
    text += cell;
  }
  const index = cells[column] ? indexAt[column] : column > 0 ? indexAt[column - 1] : 0;
  return pathAt(text, index);
}
