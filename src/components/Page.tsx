export default function Page({
  title,
  description,
  extra,
  fill = false,
  children,
}: {
  title: ReactNode;
  description?: ReactNode;
  extra?: ReactNode;
  fill?: boolean;
  children?: ReactNode;
}) {
  return (
    <div className={fill ? "flex min-h-0 flex-1 flex-col overflow-hidden px-7 pb-5 pt-6" : "flex min-h-0 flex-1 flex-col overflow-auto px-7 pb-8 pt-6"}>
      <div className={fill ? "mx-auto flex min-h-0 w-full max-w-[960px] flex-1 flex-col gap-5" : "mx-auto flex w-full max-w-[960px] flex-col gap-5"}>
        <header className="flex shrink-0 flex-wrap items-start justify-between gap-x-6 gap-y-3">
          <div className="min-w-0 flex-[1_1_280px]">
            <Typography.Title level={4} style={{ margin: 0 }}>{title}</Typography.Title>
            {description && <Typography.Paragraph type="secondary" style={{ margin: "4px 0 0" }}>{description}</Typography.Paragraph>}
          </div>
          {extra && <div className="flex shrink-0 flex-wrap items-center gap-2">{extra}</div>}
        </header>
        {children}
      </div>
    </div>
  );
}
