export function formatJson(input: string, compact = false): string {
  return JSON.stringify(JSON.parse(input), null, compact ? 0 : 2);
}
