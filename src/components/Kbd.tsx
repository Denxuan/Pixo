export default function Kbd({ children, label }: { children: ReactNode; label?: string }) {
  const { token } = antdTheme.useToken();
  return (
    <kbd
      aria-label={label}
      className="inline-flex h-5 min-w-5 shrink-0 items-center justify-center whitespace-nowrap rounded px-1.5 text-[11px] leading-none"
      style={{ fontFamily: "inherit", background: token.colorFillSecondary, color: token.colorTextSecondary, boxShadow: `inset 0 -1px 0 ${token.colorBorderSecondary}` }}
    >
      {children}
    </kbd>
  );
}
