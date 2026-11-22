export function shellSingleQuote(value: string): string | null {
  if (!value || value.length > 512 || /[\0\n\r]/.test(value)) return null;
  return `'${value.replace(/'/g, "'\\''")}'`;
}
