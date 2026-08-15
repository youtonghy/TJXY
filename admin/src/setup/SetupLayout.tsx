import { Button, ListBox, Tooltip } from '@heroui/react';
import { InlineSelect } from '@heroui-pro/react/inline-select';
import { Stepper } from '@heroui-pro/react/stepper';
import { Moon, Sun } from 'lucide-react';
import { useEffect, useId, useRef, type ReactNode } from 'react';

import { BrandMark } from '../ui/BrandMark';
import type { ClientTheme } from '../client/layout/clientTheme';

type SetupLocale = 'zh-CN' | 'en-US';

const stepLabels = {
  'en-US': ['Basic information', 'Database', 'Network', 'Administrator'],
  'zh-CN': ['基础信息', '数据库', '网络', '管理员'],
} satisfies Record<SetupLocale, string[]>;

const compactStepLabels = {
  'en-US': ['Basic', 'Database', 'Network', 'Admin'],
  'zh-CN': ['基础', '数据库', '网络', '管理员'],
} satisfies Record<SetupLocale, string[]>;

export function SetupLayout({
  children,
  footer,
  step,
  theme,
  title,
  description,
  locale,
  onLocaleChange,
  toggleTheme,
}: {
  children: ReactNode;
  footer?: ReactNode;
  step: 0 | 1 | 2 | 3 | 4;
  theme: ClientTheme;
  title: string;
  description?: string;
  locale: SetupLocale;
  onLocaleChange: (locale: SetupLocale) => void;
  toggleTheme: () => void;
}) {
  const contentRef = useRef<HTMLElement>(null);
  const titleId = useId();
  const tr = (english: string, chinese: string) => locale === 'en-US' ? english : chinese;
  const activeStep = step === 0 ? null : step;
  useEffect(() => { contentRef.current?.focus(); }, [title]);
  return (
    <div className="min-h-screen bg-background text-foreground">
      <div className="mx-auto grid min-h-screen w-full max-w-[1180px] lg:grid-cols-[240px_minmax(0,1fr)]">
        <aside className="hidden border-r border-border px-7 py-8 lg:flex lg:flex-col">
          <div className="flex items-center gap-3"><BrandMark className="size-10" priority /><div><p className="font-semibold">TJXY Setup</p><p className="text-xs text-muted">{tr('First-run configuration', '首次运行配置')}</p></div></div>
          {activeStep !== null && <SetupStepper className="mt-12" locale={locale} orientation="vertical" step={activeStep} />}
          <p className="mt-auto text-xs text-muted">TJXY 0.1.0</p>
        </aside>
        <div className="flex min-w-0 flex-col">
          <header className="flex h-16 items-center justify-between border-b border-border px-5 lg:px-8">
            <div className="flex items-center gap-3 lg:hidden"><BrandMark className="size-8" priority /><span className="text-sm font-semibold">TJXY Setup</span></div>
            <span className="hidden text-xs text-muted lg:block">{tr('Secure first-run setup', '安全的首次安装')}</span>
            <div className="flex shrink-0 items-center gap-2">
              <InlineSelect aria-label={tr('Interface language', '界面语言')} value={locale} onChange={(value) => { if (value === 'zh-CN' || value === 'en-US') onLocaleChange(value); }}>
                <InlineSelect.Trigger><InlineSelect.Value /><InlineSelect.Indicator /></InlineSelect.Trigger>
                <InlineSelect.Popover><ListBox><ListBox.Item id="zh-CN" textValue="中文">中文<ListBox.ItemIndicator /></ListBox.Item><ListBox.Item id="en-US" textValue="English">English<ListBox.ItemIndicator /></ListBox.Item></ListBox></InlineSelect.Popover>
              </InlineSelect>
              <Tooltip>
                <Button aria-label={theme === 'dark' ? tr('Switch to light theme', '切换到浅色主题') : tr('Switch to dark theme', '切换到深色主题')} isIconOnly onPress={toggleTheme} size="sm" variant="ghost">{theme === 'dark' ? <Sun aria-hidden="true" className="size-4" /> : <Moon aria-hidden="true" className="size-4" />}</Button>
                <Tooltip.Content>{theme === 'dark' ? tr('Light theme', '浅色主题') : tr('Dark theme', '深色主题')}</Tooltip.Content>
              </Tooltip>
            </div>
          </header>
          {activeStep !== null && <div className="px-5 pt-5 lg:hidden"><SetupStepper compact locale={locale} orientation="horizontal" step={activeStep} /></div>}
          <main className="flex flex-1 justify-center px-5 py-8 lg:px-10 lg:py-12">
            <section aria-labelledby={titleId} className="w-full max-w-[720px] focus:outline-none" ref={contentRef} tabIndex={-1}>
              <div className="mb-7"><p className="text-xs font-medium uppercase text-accent">{step > 0 ? tr(`Step ${String(step)} of 4`, `第 ${String(step)} 步，共 4 步`) : 'TJXY Setup'}</p><h1 className="mt-2 text-2xl font-semibold sm:text-3xl" id={titleId}>{title}</h1>{description && <p className="mt-2 max-w-2xl text-sm leading-6 text-muted">{description}</p>}</div>
              {children}
            </section>
          </main>
          {footer && <footer className="border-t border-border px-5 py-4 lg:px-10"><div className="mx-auto flex w-full max-w-[720px] justify-between gap-3">{footer}</div></footer>}
        </div>
      </div>
    </div>
  );
}

function SetupStepper({ className, compact = false, locale, orientation, step }: {
  className?: string;
  compact?: boolean;
  locale: SetupLocale;
  orientation: 'horizontal' | 'vertical';
  step: 1 | 2 | 3 | 4;
}) {
  const labels = compact ? compactStepLabels[locale] : stepLabels[locale];
  return (
    <Stepper aria-label={locale === 'en-US' ? 'Setup progress' : '安装进度'} className={className} currentStep={step - 1} orientation={orientation} size={compact ? 'sm' : 'md'}>
      {labels.map((label) => (
        <Stepper.Step key={label}>
          <Stepper.Indicator />
          <Stepper.Content><Stepper.Title>{label}</Stepper.Title></Stepper.Content>
          <Stepper.Separator />
        </Stepper.Step>
      ))}
    </Stepper>
  );
}
