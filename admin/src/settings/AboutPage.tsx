import { Card } from '@heroui/react';
import { ExternalLink, GitCommitHorizontal, Info, UserRound } from 'lucide-react';

import { BUILD_COMMIT, BUILD_VERSION } from '../api/buildVersion';
import { PageHeader } from '../ui/PageHeader';
import { useTranslate } from './i18n';

const REPOSITORY_URL = 'https://github.com/youtonghy/TJXY';
const AUTHOR = 'youtonghy';

export function AboutPage() {
  const tr = useTranslate();
  const details = [
    { label: tr('admin.about.programName'), value: 'TJXY', icon: Info },
    { label: tr('admin.about.author'), value: AUTHOR, icon: UserRound },
    { label: tr('admin.about.version'), value: BUILD_VERSION, icon: Info },
    { label: tr('admin.about.buildCommit'), value: BUILD_COMMIT, icon: GitCommitHorizontal },
  ];

  return (
    <div className="space-y-5">
      <PageHeader description={tr('admin.about.subtitle')} title={tr('admin.about.title')} />
      <Card className="max-w-3xl">
        <Card.Header className="items-start gap-3">
          <Info aria-hidden="true" className="mt-0.5 size-5 shrink-0 text-accent" />
          <div><Card.Title>{tr('admin.about.programName')}</Card.Title><Card.Description>{tr('admin.about.description')}</Card.Description></div>
        </Card.Header>
        <Card.Content className="divide-y divide-border p-0">
          {details.map(({ label, value, icon: Icon }) => <div className="flex items-center gap-4 px-6 py-4" key={label}><Icon aria-hidden="true" className="size-4 shrink-0 text-muted" /><span className="w-36 shrink-0 text-sm text-muted">{label}</span><span className="min-w-0 break-all text-sm font-medium text-foreground">{value}</span></div>)}
          <div className="flex items-center gap-4 px-6 py-4"><ExternalLink aria-hidden="true" className="size-4 shrink-0 text-muted" /><span className="w-36 shrink-0 text-sm text-muted">{tr('admin.about.repository')}</span><a className="min-w-0 break-all text-sm font-medium text-accent hover:underline" href={REPOSITORY_URL} rel="noreferrer" target="_blank">{REPOSITORY_URL}</a></div>
        </Card.Content>
      </Card>
    </div>
  );
}
