import { Avatar, Drawer, Dropdown, Spinner } from '@heroui/react';
import { LogOut, Menu as MenuIcon, TriangleAlert } from 'lucide-react';
import {
  useGetIdentity,
  useLogout,
  type UserIdentity,
} from 'ra-core';
import {
  useEffect,
  useRef,
  useState,
  type MouseEvent,
  type ReactNode,
} from 'react';
import { NavLink } from 'react-router-dom';

import { BrandMark } from '../ui/BrandMark';
import { interpolate, useTranslate } from '../settings/i18n';
import { useSystemLocale } from '../settings/SystemLocaleProvider';
import { adminNavigation } from './adminNavigation';

export function AdminShell({ children }: { children: ReactNode }) {
  const tr = useTranslate();
  const { siteTitle } = useSystemLocale();
  const [isNavigationOpen, setIsNavigationOpen] = useState(false);
  const navigationTriggerRef = useRef<HTMLButtonElement>(null);
  const shouldRestoreNavigationFocusRef = useRef(false);

  useEffect(() => {
    if (isNavigationOpen || !shouldRestoreNavigationFocusRef.current) return;
    shouldRestoreNavigationFocusRef.current = false;
    navigationTriggerRef.current?.focus();
  }, [isNavigationOpen]);

  const focusMainContent = (event: MouseEvent<HTMLAnchorElement>) => {
    event.preventDefault();
    document.getElementById('main-content')?.focus();
  };

  return (
    <div className="min-h-screen bg-background lg:grid lg:grid-cols-[15rem_minmax(0,1fr)]">
      <a className="skip-link" href="#main-content" onClick={focusMainContent}>
        {tr('admin.shell.skip')}
      </a>

      <aside className="hidden h-screen w-60 flex-col border-r border-border bg-surface lg:sticky lg:top-0 lg:flex">
        <Brand />
        <div className="min-h-0 flex-1 overflow-y-auto px-3 pb-4">
          <PrimaryNavigation />
        </div>
        <div className="border-t border-border p-3">
          <AccountIdentity />
        </div>
      </aside>

      <div className="min-w-0">
        <div className="flex h-14 items-center gap-3 border-b border-border bg-surface px-4 lg:hidden">
          <Drawer isOpen={isNavigationOpen} onOpenChange={setIsNavigationOpen}>
            <Drawer.Trigger
              aria-label={tr('admin.navigation.open')}
              className="inline-flex size-10 shrink-0 items-center justify-center rounded-md text-foreground hover:bg-default"
              ref={navigationTriggerRef}
            >
              <MenuIcon aria-hidden="true" className="size-5" />
            </Drawer.Trigger>
            <Drawer.Backdrop>
              <Drawer.Content className="max-w-[20rem]" placement="left">
                <Drawer.Dialog>
                  <Drawer.Header>
                    <Drawer.Heading>{tr('admin.navigation.title')}</Drawer.Heading>
                    <Drawer.CloseTrigger aria-label={tr('admin.navigation.close')} />
                  </Drawer.Header>
                  <Drawer.Body>
                    <PrimaryNavigation onNavigate={() => {
                      shouldRestoreNavigationFocusRef.current = true;
                      setIsNavigationOpen(false);
                    }} />
                  </Drawer.Body>
                  <Drawer.Footer>
                    <AccountIdentity />
                  </Drawer.Footer>
                </Drawer.Dialog>
              </Drawer.Content>
            </Drawer.Backdrop>
          </Drawer>
          <NavLink
            aria-label={interpolate(tr('admin.brand.home'), { title: siteTitle })}
            className="flex items-center gap-2 rounded-md text-foreground"
            to="/admin"
          >
            <BrandMark className="size-8" priority />
            <span className="text-sm font-semibold">{interpolate(tr('admin.brand.title'), { title: siteTitle })}</span>
          </NavLink>
        </div>

        <main
          className="mx-auto min-h-[calc(100vh-3.5rem)] w-full max-w-[96rem] px-4 py-5 sm:px-6 lg:min-h-screen lg:px-8 lg:py-7"
          id="main-content"
          tabIndex={-1}
        >
          {children}
        </main>
      </div>
    </div>
  );
}

function Brand() {
  const tr = useTranslate();
  const { siteTitle } = useSystemLocale();
  return (
    <NavLink
      aria-label={interpolate(tr('admin.brand.home'), { title: siteTitle })}
      className="flex h-20 items-center gap-3 rounded-md px-5 text-foreground"
      to="/admin"
    >
      <BrandMark className="size-10" priority />
      <div>
        <p className="text-base font-semibold text-foreground">{interpolate(tr('admin.brand.title'), { title: siteTitle })}</p>
        <p className="text-xs text-muted">{tr('admin.brand.subtitle')}</p>
      </div>
    </NavLink>
  );
}

function PrimaryNavigation({ onNavigate }: { onNavigate?: () => void }) {
  const tr = useTranslate();
  return (
    <nav aria-label={tr('admin.navigation.primary')} className="space-y-5">
      {adminNavigation.map((group) => (
        <div key={group.labelKey}>
          <p className="mb-1.5 px-2 text-xs font-semibold text-muted">{tr(group.labelKey)}</p>
          <ul className="space-y-1">
            {group.items.map((item) => {
              const Icon = item.icon;
              return (
                <li key={item.to}>
                  <NavLink
                    className={({ isActive }) => [
                      'flex min-h-10 items-center gap-3 rounded-md px-3 text-sm font-medium transition-colors',
                      isActive
                        ? 'bg-accent/12 text-accent'
                        : 'text-muted hover:bg-default/70 hover:text-foreground',
                    ].join(' ')}
                    end={item.to === '/admin'}
                    onClick={onNavigate}
                    to={item.to}
                  >
                    <Icon aria-hidden={true} size={17} />
                    <span>{tr(item.labelKey)}</span>
                  </NavLink>
                </li>
              );
            })}
          </ul>
        </div>
      ))}
    </nav>
  );
}

function AccountIdentity() {
  const tr = useTranslate();
  const { identity, error, isPending } = useGetIdentity({ retry: false });
  const logout = useLogout();
  const [logoutFailed, setLogoutFailed] = useState(false);

  if (isPending) {
    return (
      <div className="flex min-h-12 items-center gap-3 text-sm text-muted" role="status">
        <Spinner aria-label={tr('admin.shell.identityLoading')} size="sm" />
        {tr('admin.shell.identityLoading')}
      </div>
    );
  }

  const fullName = identityName(identity);
  if (error !== null || fullName === null) {
    return (
      <div className="flex min-h-12 items-start gap-2 text-sm text-danger" role="alert">
        <TriangleAlert aria-hidden="true" className="mt-0.5 size-4 shrink-0" />
        {tr('admin.shell.identityUnavailable')}
      </div>
    );
  }

  const handleLogout = async () => {
    setLogoutFailed(false);
    try {
      await logout();
    } catch {
      setLogoutFailed(true);
    }
  };

  return (
    <div className="space-y-2">
      <Dropdown>
        <Dropdown.Trigger
          aria-label={interpolate(tr('admin.shell.openAccount'), { name: fullName })}
          className="flex min-h-12 w-full items-center gap-3 rounded-md px-2 text-left hover:bg-default/70"
        >
          <Avatar aria-label="Administrator avatar" size="sm">
            {identity?.avatar !== undefined && <Avatar.Image alt="" src={identity.avatar} />}
            <Avatar.Fallback>{fullName.charAt(0)}</Avatar.Fallback>
          </Avatar>
          <span className="min-w-0 flex-1">
            <span className="block truncate text-sm font-medium text-foreground">{fullName}</span>
            <span className="block text-xs text-muted">{tr('admin.shell.administrator')}</span>
          </span>
        </Dropdown.Trigger>
        <Dropdown.Popover placement="top start">
          <Dropdown.Menu
            aria-label={tr('admin.shell.accountActions')}
            onAction={(key) => {
              if (key === 'sign-out') void handleLogout();
            }}
          >
            <Dropdown.Item id="sign-out" textValue={tr('admin.shell.signOut')} variant="danger">
              <LogOut aria-hidden="true" className="size-4" />
              {tr('admin.shell.signOut')}
            </Dropdown.Item>
          </Dropdown.Menu>
        </Dropdown.Popover>
      </Dropdown>
      {logoutFailed && <p className="text-xs text-danger" role="alert">{tr('admin.shell.signOutFailed')}</p>}
    </div>
  );
}

function identityName(identity: UserIdentity | undefined): string | null {
  const fullName = identity?.fullName?.trim();
  return fullName === undefined || fullName.length === 0 ? null : fullName;
}
