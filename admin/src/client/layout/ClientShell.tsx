/* eslint-disable @typescript-eslint/no-unnecessary-condition */
import { Avatar, Button, Drawer, Dropdown, Toolbar, Tooltip } from '@heroui/react';
import { Home, Library, LogOut, Menu, Moon, Search, Sun, Trophy, UserRound } from 'lucide-react';
import { useRef, useState, type ReactNode } from 'react';
import { Link, NavLink, useNavigate } from 'react-router-dom';
import { BrandMark } from '../../ui/BrandMark';
import { useClientAuth } from '../auth/ClientAuthContext';
import { useClientTheme } from './clientTheme';

const links = [
  { to: '/app/', label: 'Home', icon: Home },
  { to: '/app/libraries', label: 'Libraries', icon: Library },
  { to: '/app/search', label: 'Search', icon: Search },
  { to: '/app/rankings', label: 'Rankings', icon: Trophy },
];

export function ClientShell({ children }: { children: ReactNode }) {
  const { user, signOut } = useClientAuth();
  const navigate = useNavigate();
  const { theme, toggleTheme } = useClientTheme();
  const [open, setOpen] = useState(false);
  const trigger = useRef<HTMLButtonElement>(null);
  return (
    <div className="min-h-screen bg-background">
      <header className="sticky top-0 z-20 border-b border-border bg-surface/95 backdrop-blur">
        <Toolbar aria-label="TJXY navigation" className="mx-auto flex h-16 max-w-[96rem] items-center gap-2 border-0 bg-transparent px-4 shadow-none sm:px-6 lg:gap-5 lg:px-8">
          <Drawer isOpen={open} onOpenChange={setOpen}>
            <Drawer.Trigger
              aria-label="Open navigation"
              className="inline-flex size-10 items-center justify-center rounded-lg text-muted hover:bg-default hover:text-foreground lg:hidden"
              ref={trigger}
            >
              <Menu aria-hidden="true" className="size-5" />
            </Drawer.Trigger>
            <Drawer.Backdrop variant="blur">
              <Drawer.Content className="max-w-[20rem]" placement="left">
                <Drawer.Dialog aria-label="Browse TJXY">
                  <Drawer.Header className="border-b border-border">
                    <div>
                      <Drawer.Heading>Browse TJXY</Drawer.Heading>
                      <p className="mt-1 text-xs text-muted">Your personal media library</p>
                    </div>
                    <Drawer.CloseTrigger aria-label="Close navigation" />
                  </Drawer.Header>
                  <Drawer.Body className="px-3 py-4">
                    <nav aria-label="Mobile navigation">
                      <ClientNavigation mobile onNavigate={() => { setOpen(false); }} />
                    </nav>
                  </Drawer.Body>
                </Drawer.Dialog>
              </Drawer.Content>
            </Drawer.Backdrop>
          </Drawer>
          <Link
            aria-label="TJXY home"
            className="flex items-center gap-2 rounded-md px-1 text-base font-semibold text-foreground"
            to="/app/"
          >
            <BrandMark className="size-8" priority />
            <span>TJXY</span>
          </Link>
          <nav aria-label="Primary" className="hidden items-center gap-1 lg:flex">
            <ClientNavigation />
          </nav>
          <div className="ml-auto flex items-center gap-1">
            <Tooltip>
              <Tooltip.Trigger>
                <Button
                  aria-label={`Switch to ${theme === 'dark' ? 'light' : 'dark'} theme`}
                  isIconOnly
                  onPress={toggleTheme}
                  size="sm"
                  variant="ghost"
                >
                  {theme === 'dark'
                    ? <Sun aria-hidden="true" className="size-4" />
                    : <Moon aria-hidden="true" className="size-4" />}
                </Button>
              </Tooltip.Trigger>
              <Tooltip.Content>{theme === 'dark' ? 'Use light theme' : 'Use dark theme'}</Tooltip.Content>
            </Tooltip>
            <Dropdown>
              <Dropdown.Trigger
                aria-label={`Open account menu for ${user?.Name ?? 'user'}`}
                className="inline-flex items-center gap-2 rounded-lg px-2 py-1 hover:bg-default"
              >
                <Avatar size="sm"><Avatar.Fallback>{user?.Name?.slice(0, 1).toUpperCase()}</Avatar.Fallback></Avatar>
                <span className="hidden text-sm sm:inline">{user?.Name}</span>
              </Dropdown.Trigger>
              <Dropdown.Popover>
                <Dropdown.Menu onAction={(key) => {
                  if (key === 'profile') void navigate('/app/profile');
                  if (key === 'logout') void signOut();
                }}>
                  <Dropdown.Item id="profile"><UserRound className="size-4" />Profile & stats</Dropdown.Item>
                  <Dropdown.Item id="logout"><LogOut className="size-4" />Sign out</Dropdown.Item>
                </Dropdown.Menu>
              </Dropdown.Popover>
            </Dropdown>
          </div>
        </Toolbar>
      </header>
      <main className="mx-auto w-full max-w-[96rem] px-4 py-6 sm:px-6 lg:px-8">{children}</main>
    </div>
  );
}

function ClientNavigation({ mobile = false, onNavigate }: { mobile?: boolean; onNavigate?: () => void }) {
  return (
    <div className={mobile ? 'flex flex-col gap-1' : 'contents'}>
      {links.map(({ to, label, icon: Icon }) => (
        <NavLink
          className={({ isActive }) => [
            'inline-flex items-center gap-3 rounded-lg text-sm font-medium transition-colors',
            mobile ? 'w-full px-3 py-3' : 'px-3 py-2',
            isActive ? 'bg-accent/12 text-accent' : 'text-muted hover:bg-default hover:text-foreground',
          ].join(' ')}
          end={to === '/app/'}
          key={to}
          onClick={onNavigate}
          to={to}
        >
          <Icon aria-hidden="true" className={mobile ? 'size-5' : 'size-4'} />
          {label}
        </NavLink>
      ))}
    </div>
  );
}
