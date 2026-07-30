/* eslint-disable @typescript-eslint/no-unnecessary-condition */
import { Avatar, Drawer, Dropdown } from '@heroui/react';
import { Home, Library, LogOut, Menu, Search } from 'lucide-react';
import { useRef, useState, type ReactNode } from 'react';
import { NavLink } from 'react-router-dom';
import { useClientAuth } from '../auth/ClientAuthContext';

const links = [
  { to: '/app/', label: 'Home', icon: Home },
  { to: '/app/libraries', label: 'Libraries', icon: Library },
  { to: '/app/search', label: 'Search', icon: Search },
];

export function ClientShell({ children }: { children: ReactNode }) {
  const { user, signOut } = useClientAuth();
  const [open, setOpen] = useState(false);
  const trigger = useRef<HTMLButtonElement>(null);
  return (
    <div className="min-h-screen bg-background">
      <header className="sticky top-0 z-20 border-b border-border bg-surface/95 backdrop-blur">
        <div className="mx-auto flex h-16 max-w-[96rem] items-center gap-4 px-4 sm:px-6 lg:px-8">
          <Drawer isOpen={open} onOpenChange={setOpen}>
            <Drawer.Trigger
              aria-label="Open navigation"
              className="inline-flex size-10 items-center justify-center rounded-lg hover:bg-default lg:hidden"
              ref={trigger}
            >
              <Menu className="size-5" />
            </Drawer.Trigger>
            <Drawer.Backdrop>
              <Drawer.Content className="max-w-[20rem]" placement="left">
                <Drawer.Dialog>
                  <Drawer.Header>
                    <Drawer.Heading>Navigation</Drawer.Heading>
                    <Drawer.CloseTrigger aria-label="Close navigation" />
                  </Drawer.Header>
                  <Drawer.Body>
                    <ClientNavigation onNavigate={() => { setOpen(false); }} />
                  </Drawer.Body>
                </Drawer.Dialog>
              </Drawer.Content>
            </Drawer.Backdrop>
          </Drawer>
          <span className="text-base font-semibold text-foreground">TJXY</span>
          <nav aria-label="Primary" className="hidden items-center gap-1 lg:flex">
            <ClientNavigation />
          </nav>
          <div className="ml-auto">
            <Dropdown>
              <Dropdown.Trigger
                aria-label={`Open account menu for ${user?.Name ?? 'user'}`}
                className="inline-flex items-center gap-2 rounded-lg px-2 py-1 hover:bg-default"
              >
                <Avatar size="sm"><Avatar.Fallback>{user?.Name?.slice(0, 1).toUpperCase()}</Avatar.Fallback></Avatar>
                <span className="hidden text-sm sm:inline">{user?.Name}</span>
              </Dropdown.Trigger>
              <Dropdown.Popover>
                <Dropdown.Menu onAction={(key) => { if (key === 'logout') void signOut(); }}>
                  <Dropdown.Item id="logout"><LogOut className="size-4" />Sign out</Dropdown.Item>
                </Dropdown.Menu>
              </Dropdown.Popover>
            </Dropdown>
          </div>
        </div>
      </header>
      <main className="mx-auto w-full max-w-[96rem] px-4 py-6 sm:px-6 lg:px-8">{children}</main>
    </div>
  );
}

function ClientNavigation({ onNavigate }: { onNavigate?: () => void }) {
  return <>{links.map(({ to, label, icon: Icon }) => <NavLink className={({ isActive }) => `inline-flex items-center gap-2 rounded-lg px-3 py-2 text-sm ${isActive ? 'bg-accent/12 text-accent' : 'text-muted hover:bg-default'}`} key={to} onClick={onNavigate} to={to}><Icon className="size-4" />{label}</NavLink>)}</>;
}
