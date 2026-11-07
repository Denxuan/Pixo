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
    <div className="flex h-full">
      <style>{motion}</style>
      <div className="box-border flex w-[232px] flex-col gap-2 px-5 pt-6" style={{ background: token.colorBgLayout, borderInlineEnd: `1px solid ${token.colorBorderSecondary}` }}>
        <img className="boot-mark h-10 w-10 rounded-xl" src="/pixo-icon.png" alt="" />
        <p className="boot-name m-0 text-xl font-semibold leading-[1.2] tracking-[-0.02em]" style={{ color: token.colorText }}>Pixo</p>
        <p className="boot-line m-0 text-sm leading-[1.4]" style={{ color: token.colorTextSecondary }}>{t("zdx.text.app.tagline")}</p>
      </div>
    </div>
  );
}
