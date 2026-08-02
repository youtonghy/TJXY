/* eslint-disable @typescript-eslint/no-unnecessary-condition */
import { Avatar, Button, Dropdown, Tooltip } from '@heroui/react';
import { Navbar } from '@heroui-pro/react/navbar';
import { Home, Library, LogOut, Moon, Search, Sun, Trophy, UserRound } from 'lucide-react';
import { useState, type ReactNode } from 'react';
import { Link, useLocation, useNavigate } from 'react-router-dom';
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
  const { pathname } = useLocation();
  const { theme, toggleTheme } = useClientTheme();
  const [open, setOpen] = useState(false);
  return (
    <div className="min-h-screen bg-background">
      <Navbar
        aria-label="TJXY navigation"
        className="border-b border-border bg-surface/95 backdrop-blur"
        height="4rem"
        isMenuOpen={open}
        maxWidth="full"
        navigate={(href) => { void navigate(href); }}
        onMenuOpenChange={setOpen}
        position="sticky"
      >
        <Navbar.Header className="max-w-[96rem] gap-2 px-4 sm:px-6 lg:gap-5 lg:px-8">
          <Navbar.MenuToggle className="lg:hidden" srLabel="Open navigation" />
          <Navbar.Brand>
            <Link
              aria-label="TJXY home"
              className="flex items-center gap-2 rounded-md px-1 text-base font-semibold text-foreground"
              to="/app/"
            >
              <BrandMark className="size-8" priority />
              <span>TJXY</span>
            </Link>
          </Navbar.Brand>
          <Navbar.Content className="hidden lg:flex">
            <ClientNavigation pathname={pathname} />
          </Navbar.Content>
          <Navbar.Spacer />
          <Navbar.Content>
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
          </Navbar.Content>
        </Navbar.Header>
        <Navbar.Menu aria-label="Mobile navigation" role="navigation">
          <ClientNavigation mobile pathname={pathname} />
        </Navbar.Menu>
      </Navbar>
      <main className="mx-auto w-full max-w-[96rem] px-4 py-6 sm:px-6 lg:px-8">{children}</main>
    </div>
  );
}

function ClientNavigation({ mobile = false, pathname }: { mobile?: boolean; pathname: string }) {
  return (
    links.map(({ to, label, icon: Icon }) => {
      const Item = mobile ? Navbar.MenuItem : Navbar.Item;
      return (
        <Item
          className={mobile ? 'gap-3' : 'gap-2'}
          href={to}
          isCurrent={isCurrentRoute(pathname, to)}
          key={to}
        >
          <Icon aria-hidden="true" className={mobile ? 'size-5' : 'size-4'} />
          <Navbar.Label>{label}</Navbar.Label>
        </Item>
      );
    })
  );
}

function isCurrentRoute(pathname: string, to: string) {
  if (to === '/app/') return pathname === '/app' || pathname === '/app/';
  return pathname === to || pathname.startsWith(`${to}/`);
}
