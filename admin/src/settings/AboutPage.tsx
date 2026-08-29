import { Card } from '@heroui/react';
import { ExternalLink, GitCommitHorizontal, Package, UserRound } from 'lucide-react';

import { BUILD_COMMIT, BUILD_VERSION } from '../api/buildVersion';
import { BrandMark } from '../ui/BrandMark';
import { PageHeader } from '../ui/PageHeader';
import { useTranslate } from './i18n';

const REPOSITORY_URL = 'https://github.com/youtonghy/TJXY';
const AUTHOR = 'youtonghy';

export function AboutPage() {
  const tr = useTranslate();
  const details = [
    { label: tr('admin.about.author'), value: AUTHOR, icon: UserRound },
    { label: tr('admin.about.version'), value: BUILD_VERSION, icon: Package },
    { label: tr('admin.about.buildCommit'), value: BUILD_COMMIT, icon: GitCommitHorizontal },
  ];

  return (
    <div className="space-y-5">
      <PageHeader description={tr('admin.about.subtitle')} title={tr('admin.about.title')} />
      <section aria-label={tr('admin.about.programName')} className="grid max-w-4xl gap-4 md:grid-cols-2">
        <Card className="md:col-span-2">
          <Card.Content className="flex-row items-center gap-5 p-6 sm:p-8">
            <div className="flex size-16 shrink-0 items-center justify-center rounded-lg bg-accent-soft sm:size-20">
              <BrandMark className="size-12 sm:size-14" priority />
            </div>
            <div className="min-w-0">
              <p className="text-sm font-medium text-accent">{tr('admin.about.programName')}</p>
              <h2 className="mt-1 text-2xl font-semibold text-foreground">TJXY</h2>
              <p className="mt-2 max-w-2xl text-sm leading-6 text-muted">{tr('admin.about.description')}</p>
            </div>
          </Card.Content>
        </Card>

        {details.map(({ label, value, icon: Icon }) => (
          <Card className="min-h-32" key={label}>
            <Card.Header className="flex-row items-center gap-3 pb-0">
              <span className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-default text-muted">
                <Icon aria-hidden="true" className="size-4" />
              </span>
              <Card.Description>{label}</Card.Description>
            </Card.Header>
            <Card.Content className="mt-auto pt-4">
              <p className="break-all text-lg font-semibold text-foreground">{value}</p>
            </Card.Content>
          </Card>
        ))}

        <Card className="min-h-32">
          <Card.Header className="flex-row items-center gap-3 pb-0">
            <span className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-default text-muted">
              <ExternalLink aria-hidden="true" className="size-4" />
            </span>
            <Card.Description>{tr('admin.about.repository')}</Card.Description>
          </Card.Header>
          <Card.Content className="mt-auto pt-4">
            <a className="inline-flex max-w-full items-center gap-2 break-all text-sm font-semibold text-accent hover:underline" href={REPOSITORY_URL} rel="noreferrer" target="_blank">
              {REPOSITORY_URL}
              <ExternalLink aria-hidden="true" className="size-4 shrink-0" />
            </a>
          </Card.Content>
        </Card>
      </section>
    </div>
  );
}
