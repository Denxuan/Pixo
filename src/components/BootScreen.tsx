const motion = `
.boot-mark,
.boot-name,
.boot-line {
  animation: boot-rise 460ms cubic-bezier(0.2, 0.8, 0.2, 1) both;
}
.boot-mark {
  animation-name: boot-mark;
}
.boot-name {
  animation-delay: 70ms;
}
.boot-line {
  animation-delay: 120ms;
}
@keyframes boot-mark {
  from { opacity: 0; transform: scale(0.94); }
  to { opacity: 1; transform: scale(1); }
}
@keyframes boot-rise {
  from { opacity: 0; transform: translateY(8px); }
  to { opacity: 1; transform: none; }
}
@media (prefers-reduced-motion: reduce) {
  .boot-mark,
  .boot-name,
  .boot-line {
    animation: none;
  }
}
`;

export default function BootScreen() {
  const { t } = useTranslation();
  const { token } = antdTheme.useToken();

  return (
    <div className="flex h-full" style={{ background: token.colorBgLayout }}>
      <style>{motion}</style>
      <div className="box-border flex w-[216px] shrink-0 items-center gap-2.5 self-start px-5 pt-5">
        <img className="boot-mark h-8 w-8 rounded-lg" src="/pixo-icon.png" alt="" />
        <div className="min-w-0">
          <p className="boot-name m-0 text-[15px] font-semibold leading-tight" style={{ color: token.colorText }}>Pixo</p>
          <p className="boot-line m-0 text-xs leading-snug" style={{ color: token.colorTextTertiary }}>{t("zdx.text.app.tagline")}</p>
        </div>
      </div>
      <div className="box-border flex min-w-0 flex-1 py-2 pr-2">
        <div className="flex-1" style={{ background: token.colorBgContainer, border: `1px solid ${token.colorBorderSecondary}`, borderRadius: token.borderRadiusLG }} />
      </div>
    </div>
  );
}
