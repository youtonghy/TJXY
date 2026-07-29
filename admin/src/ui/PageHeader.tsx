import { ChevronRight } from 'lucide-react';
import { useEffect, type ReactNode } from 'react';
import { Link } from 'react-router-dom';

export interface BreadcrumbItem {
  label: string;
  to?: string;
}

export interface PageHeaderProps {
  title: string;
  description?: string;
  breadcrumbs?: readonly BreadcrumbItem[];
  actions?: ReactNode;
}

export function PageHeader({ title, description, breadcrumbs = [], actions }: PageHeaderProps) {
  useEffect(() => {
    document.title = `${title} | TJXY Admin`;
  }, [title]);

  return (
    <header className="flex flex-col gap-4 border-b border-default/20 pb-5">
      {breadcrumbs.length > 0 && (
        <nav aria-label="Breadcrumb" className="min-w-0 text-sm text-muted">
          <ol className="flex min-w-0 flex-wrap items-center gap-1.5">
            {breadcrumbs.map((item, index) => {
              const isCurrent = index === breadcrumbs.length - 1;
              return (
                <li
                  className="flex min-w-0 items-center gap-1.5"
                  key={`${item.label}-${String(index)}`}
                >
                  {index > 0 && <ChevronRight aria-hidden="true" className="size-3.5 shrink-0" />}
                  {item.to === undefined ? (
                    <span aria-current={isCurrent ? 'page' : undefined} className="truncate">
                      {item.label}
                    </span>
                  ) : (
                    <Link className="truncate text-accent hover:underline" to={item.to}>
                      {item.label}
                    </Link>
                  )}
                </li>
              );
            })}
          </ol>
        </nav>
      )}

      <div className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div className="min-w-0 max-w-3xl">
          <h1 className="text-2xl font-semibold text-foreground">{title}</h1>
          {description !== undefined && (
            <p className="mt-1 max-w-2xl text-sm leading-6 text-muted">{description}</p>
          )}
        </div>
        <div
          className="flex min-h-9 shrink-0 flex-wrap items-center gap-2"
          data-testid="page-header-actions"
        >
          {actions}
        </div>
      </div>
    </header>
  );
}
