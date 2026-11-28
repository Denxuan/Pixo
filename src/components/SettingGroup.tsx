export function SettingGroup({ title, children }: { title?: ReactNode; children: ReactNode }) {
  const { token } = antdTheme.useToken();
  return (
    <section className="flex flex-col gap-2">
      {title && <Typography.Text className="px-1" style={{ fontSize: token.fontSizeSM, fontWeight: 600, color: token.colorTextSecondary }}>{title}</Typography.Text>}
      <div className="overflow-hidden" style={{ background: token.colorBgElevated, border: `1px solid ${token.colorBorder}`, borderRadius: token.borderRadiusLG }}>
        {children}
      </div>
    </section>
  );
}

export function SettingRow({
  label,
  description,
  stacked = false,
  children,
}: {
  label: ReactNode;
  description?: ReactNode;
  stacked?: boolean;
  children?: ReactNode;
}) {
  const { token } = antdTheme.useToken();
  const text = (
    <div className={stacked ? "min-w-0" : "min-w-0 flex-[1_1_220px]"}>
      <Typography.Text>{label}</Typography.Text>
      {description && <div style={{ marginTop: 2, fontSize: token.fontSizeSM, lineHeight: 1.5, color: token.colorTextSecondary }}>{description}</div>}
    </div>
  );
  return (
    <div className="border-0 border-t border-solid px-4 py-3 first:border-t-0" style={{ borderColor: token.colorBorderSecondary }}>
      {stacked ? (
        <div className="flex flex-col gap-3">
          {text}
          {children}
        </div>
      ) : (
        <div className="flex flex-wrap items-center justify-between gap-x-6 gap-y-2">
          {text}
          {children && <div className="flex max-w-full shrink-0 items-center justify-end">{children}</div>}
        </div>
      )}
    </div>
  );
}
