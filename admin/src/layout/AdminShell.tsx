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
import { adminNavigation } from './adminNavigation';

export function AdminShell({ children }: { children: ReactNode }) {
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
        Skip to content
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
              aria-label="Open navigation"
              className="inline-flex size-10 shrink-0 items-center justify-center rounded-md text-foreground hover:bg-default"
              ref={navigationTriggerRef}
            >
              <MenuIcon aria-hidden="true" className="size-5" />
            </Drawer.Trigger>
            <Drawer.Backdrop>
              <Drawer.Content className="max-w-[20rem]" placement="left">
                <Drawer.Dialog>
                  <Drawer.Header>
                    <Drawer.Heading>Navigation</Drawer.Heading>
                    <Drawer.CloseTrigger aria-label="Close navigation" />
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
            aria-label="TJXY Admin home"
            className="flex items-center gap-2 rounded-md text-foreground"
            to="/admin"
          >
            <BrandMark className="size-8" priority />
            <span className="text-sm font-semibold">TJXY Admin</span>
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
  return (
    <NavLink
      aria-label="TJXY Admin home"
      className="flex h-20 items-center gap-3 rounded-md px-5 text-foreground"
      to="/admin"
    >
      <BrandMark className="size-10" priority />
      <div>
        <p className="text-base font-semibold text-foreground">TJXY Admin</p>
        <p className="text-xs text-muted">Administrator workspace</p>
      </div>
    </NavLink>
  );
}

function PrimaryNavigation({ onNavigate }: { onNavigate?: () => void }) {
  return (
    <nav aria-label="Primary" className="space-y-5">
      {adminNavigation.map((group) => (
        <div key={group.label}>
          <p className="mb-1.5 px-2 text-xs font-semibold text-muted">{group.label}</p>
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
                    onClick={onNavigate}
                    to={item.to}
                  >
                    <Icon aria-hidden={true} size={17} />
                    <span>{item.label}</span>
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
  const { identity, error, isPending } = useGetIdentity({ retry: false });
  const logout = useLogout();
  const [logoutFailed, setLogoutFailed] = useState(false);

  if (isPending) {
    return (
      <div className="flex min-h-12 items-center gap-3 text-sm text-muted" role="status">
        <Spinner aria-label="Loading administrator identity" size="sm" />
        Loading identity
      </div>
    );
  }

  const fullName = identityName(identity);
  if (error !== null || fullName === null) {
    return (
      <div className="flex min-h-12 items-start gap-2 text-sm text-danger" role="alert">
        <TriangleAlert aria-hidden="true" className="mt-0.5 size-4 shrink-0" />
        Administrator identity unavailable
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
          aria-label={`Open account menu for ${fullName}`}
          className="flex min-h-12 w-full items-center gap-3 rounded-md px-2 text-left hover:bg-default/70"
        >
          <Avatar aria-label="Administrator avatar" size="sm">
            {identity?.avatar !== undefined && <Avatar.Image alt="" src={identity.avatar} />}
            <Avatar.Fallback>{fullName.charAt(0)}</Avatar.Fallback>
          </Avatar>
          <span className="min-w-0 flex-1">
            <span className="block truncate text-sm font-medium text-foreground">{fullName}</span>
            <span className="block text-xs text-muted">Administrator</span>
          </span>
        </Dropdown.Trigger>
        <Dropdown.Popover placement="top start">
          <Dropdown.Menu
            aria-label="Account actions"
            onAction={(key) => {
              if (key === 'sign-out') void handleLogout();
            }}
          >
            <Dropdown.Item id="sign-out" textValue="Sign out" variant="danger">
              <LogOut aria-hidden="true" className="size-4" />
              Sign out
            </Dropdown.Item>
          </Dropdown.Menu>
        </Dropdown.Popover>
      </Dropdown>
      {logoutFailed && <p className="text-xs text-danger" role="alert">Sign out failed. Try again.</p>}
    </div>
  );
}

function identityName(identity: UserIdentity | undefined): string | null {
  const fullName = identity?.fullName?.trim();
  return fullName === undefined || fullName.length === 0 ? null : fullName;
}
