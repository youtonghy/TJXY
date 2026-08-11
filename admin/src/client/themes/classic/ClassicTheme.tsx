import { Avatar, Button, Dropdown, Tooltip } from '@heroui/react';
import { Navbar } from '@heroui-pro/react/navbar';
import { LogOut, Moon, Sun, UserRound } from 'lucide-react';
import { useState } from 'react';
import { Link } from 'react-router-dom';
import { useTranslate } from '../../../settings/i18n';
import type { ThemeLoginFrameProps, ThemeShellProps } from '../types';

export function ClassicThemeShell(props: ThemeShellProps) {
  const tr = useTranslate();
  const [open, setOpen] = useState(false);
  const wide = props.options.contentWidth === 'wide';
  return (
    <div className="min-h-screen bg-background">
      <Navbar
        aria-label={tr(`${props.siteTitle} navigation`, `${props.siteTitle} 导航`)}
        className="border-b border-border bg-surface/95 backdrop-blur"
        height="4rem"
        isMenuOpen={open}
        maxWidth="full"
        navigate={props.onNavigate}
        onMenuOpenChange={setOpen}
        position="sticky"
      >
        <Navbar.Header className="max-w-[96rem] gap-2 px-4 sm:px-6 lg:gap-5 lg:px-8">
          <Navbar.MenuToggle className="lg:hidden" srLabel={tr('Open navigation', '打开导航')} />
          <Navbar.Brand>
            <Link aria-label={tr(`${props.siteTitle} home`, `${props.siteTitle} 首页`)} className="flex items-center gap-2 rounded-md px-1 text-base font-semibold text-foreground" to="/app/">
              <img alt="" aria-hidden="true" className="size-8 shrink-0 object-contain" height="32" src={props.logoUrl} width="32" />
              <span>{props.siteTitle}</span>
            </Link>
          </Navbar.Brand>
          <Navbar.Content className="hidden lg:flex"><ClassicNavigation {...props} /></Navbar.Content>
          <Navbar.Spacer />
          <Navbar.Content>
            {props.announcements}
            <ColorModeButton {...props} />
            <AccountMenu {...props} />
          </Navbar.Content>
        </Navbar.Header>
        <Navbar.Menu aria-label={tr('Mobile navigation', '移动端导航')} role="navigation">
          <ClassicNavigation {...props} mobile />
        </Navbar.Menu>
      </Navbar>
      <main className={`mx-auto w-full px-4 py-6 sm:px-6 lg:px-8 ${wide ? 'max-w-[112rem]' : 'max-w-[96rem]'}`}>{props.children}</main>
    </div>
  );
}

function ClassicNavigation(props: ThemeShellProps & { mobile?: boolean }) {
  return props.navigation.map(({ to, label, icon: Icon }) => {
    const Item = props.mobile ? Navbar.MenuItem : Navbar.Item;
    return (
      <Item className={props.mobile ? 'gap-3' : 'gap-2'} href={to} isCurrent={isCurrentRoute(props.pathname, to)} key={to}>
        <Icon aria-hidden className={props.mobile ? 'size-5' : 'size-4'} />
        <Navbar.Label>{label}</Navbar.Label>
      </Item>
    );
  });
}

function ColorModeButton(props: ThemeShellProps) {
  const tr = useTranslate();
  const dark = props.colorMode === 'dark';
  return (
    <Tooltip>
      <Tooltip.Trigger>
        <Button aria-label={dark ? tr('Switch to light mode', '切换到浅色模式') : tr('Switch to dark mode', '切换到深色模式')} isIconOnly onPress={props.onToggleColorMode} size="sm" variant="ghost">
          {dark ? <Sun aria-hidden className="size-4" /> : <Moon aria-hidden className="size-4" />}
        </Button>
      </Tooltip.Trigger>
      <Tooltip.Content>{dark ? tr('Use light mode', '使用浅色模式') : tr('Use dark mode', '使用深色模式')}</Tooltip.Content>
    </Tooltip>
  );
}

function AccountMenu(props: ThemeShellProps) {
  const tr = useTranslate();
  return (
    <Dropdown>
      <Dropdown.Trigger aria-label={tr(`Open account menu for ${props.userName}`, `打开 ${props.userName} 的账户菜单`)} className="inline-flex items-center gap-2 rounded-lg px-2 py-1 hover:bg-default">
        <Avatar size="sm"><Avatar.Fallback>{props.userName.slice(0, 1).toUpperCase()}</Avatar.Fallback></Avatar>
        <span className="hidden text-sm sm:inline">{props.userName}</span>
      </Dropdown.Trigger>
      <Dropdown.Popover>
        <Dropdown.Menu onAction={(key) => { if (key === 'profile') props.onNavigate('/app/profile'); if (key === 'logout') props.onSignOut(); }}>
          <Dropdown.Item id="profile"><UserRound className="size-4" />{tr('Profile & stats', '个人资料与统计')}</Dropdown.Item>
          <Dropdown.Item id="logout"><LogOut className="size-4" />{tr('Sign out', '退出登录')}</Dropdown.Item>
        </Dropdown.Menu>
      </Dropdown.Popover>
    </Dropdown>
  );
}

export function ClassicLoginFrame(props: ThemeLoginFrameProps) {
  return (
    <div className="min-h-screen bg-background px-4 py-10">
      <main className="mx-auto flex min-h-[calc(100vh-5rem)] max-w-md items-center justify-center">
        <section className="relative w-full rounded-lg border border-border bg-surface p-7 shadow-sm sm:p-9">
          <div className="absolute right-5 top-5">{props.actions}</div>
          <div className="mb-8 flex items-center gap-3">
            <img alt="" aria-hidden="true" className="size-11 object-contain" height="44" src={props.logoUrl} width="44" />
            <div><p className="text-base font-semibold text-foreground">{props.siteTitle}</p><p className="text-sm text-muted">{props.siteSubtitle}</p></div>
          </div>
          {props.children}
        </section>
      </main>
    </div>
  );
}

function isCurrentRoute(pathname: string, to: string) {
  if (to === '/app/') return pathname === '/app' || pathname === '/app/';
  return pathname === to || pathname.startsWith(`${to}/`);
}
