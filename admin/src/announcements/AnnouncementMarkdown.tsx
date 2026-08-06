import ReactMarkdown from 'react-markdown';

export function AnnouncementMarkdown({ children }: { children: string }) {
  return (
    <div className="announcement-markdown min-w-0 text-sm leading-7 text-foreground">
      <ReactMarkdown
        components={{
          a: ({ children: label, href }) => safeHref(href) === null
            ? <span>{label}</span>
            : <a className="text-accent underline underline-offset-2" href={href} rel="noreferrer noopener" target="_blank">{label}</a>,
          img: () => null,
        }}
        skipHtml
      >
        {children}
      </ReactMarkdown>
    </div>
  );
}

function safeHref(value: string | undefined): string | null {
  if (value === undefined || value.startsWith('//')) return null;
  if (value.startsWith('/')) return value;
  try {
    const url = new URL(value);
    return url.protocol === 'http:' || url.protocol === 'https:' ? value : null;
  } catch {
    return null;
  }
}
