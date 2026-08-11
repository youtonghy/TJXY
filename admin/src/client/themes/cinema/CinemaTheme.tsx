import { Avatar, Button, Drawer, Dropdown, Tooltip } from '@heroui/react';
import { LogOut, Menu, Moon, Sun, UserRound } from 'lucide-react';
import { useState } from 'react';
import { NavLink } from 'react-router-dom';
import { useTranslate } from '../../../settings/i18n';
import type { ThemeLoginFrameProps, ThemeShellProps } from '../types';

export function CinemaThemeShell(props: ThemeShellProps) {
  const tr = useTranslate();
  const [navigationOpen, setNavigationOpen] = useState(false);
  const wide = props.options.contentWidth === 'wide';
  const compact = props.options.density === 'compact';
  return (
    <div className="min-h-screen bg-background lg:grid lg:grid-cols-[17rem_minmax(0,1fr)]">
      <aside className="cinema-rail hidden h-screen flex-col border-r border-border lg:sticky lg:top-0 lg:flex">
        <CinemaBrand {...props} />
        <CinemaNavigation {...props} compact={compact} />
        <div className="mt-auto flex items-center justify-between border-t border-border p-4">
          <AccountMenu {...props} />
          <ColorModeButton {...props} />
        </div>
      </aside>
      <div className="min-w-0">
        <header className="cinema-mobile-header sticky top-0 z-40 flex h-16 items-center gap-3 border-b border-border px-4 backdrop-blur lg:hidden">
          <Drawer isOpen={navigationOpen} onOpenChange={setNavigationOpen}>
            <Drawer.Trigger aria-label={tr('Open navigation', '打开导航')} className="inline-flex size-10 items-center justify-center rounded-md hover:bg-default"><Menu aria-hidden className="size-5" /></Drawer.Trigger>
            <Drawer.Backdrop><Drawer.Content className="max-w-[19rem]" placement="left"><Drawer.Dialog><Drawer.Header><Drawer.Heading>{props.siteTitle}</Drawer.Heading><Drawer.CloseTrigger /></Drawer.Header><Drawer.Body><CinemaNavigation {...props} compact={false} onNavigationComplete={() => { setNavigationOpen(false); }} /></Drawer.Body></Drawer.Dialog></Drawer.Content></Drawer.Backdrop>
          </Drawer>
          <NavLink className="flex min-w-0 items-center gap-2" to="/app/"><img alt="" className="size-8 object-contain" height="32" src={props.logoUrl} width="32" /><span className="truncate font-semibold">{props.siteTitle}</span></NavLink>
          <div className="ml-auto flex items-center gap-1">{props.announcements}<ColorModeButton {...props} /><AccountMenu {...props} compact /></div>
        </header>
        <main className={`mx-auto w-full ${wide ? 'max-w-[112rem]' : 'max-w-[92rem]'} ${compact ? 'px-4 py-4 lg:px-6' : 'px-4 py-6 sm:px-6 lg:px-10 lg:py-8'}`}>{props.children}</main>
      </div>
    </div>
  );
}

function CinemaBrand(props: ThemeShellProps) {
  return <NavLink className="flex h-24 items-center gap-3 px-5" to="/app/"><img alt="" className="size-11 object-contain" height="44" src={props.logoUrl} width="44" /><div className="min-w-0"><p className="truncate text-base font-semibold">{props.siteTitle}</p><p className="text-xs uppercase text-muted">Media room</p></div></NavLink>;
}

function CinemaNavigation(props: ThemeShellProps & { compact: boolean; onNavigationComplete?: () => void }) {
  return <nav aria-label="Client" className={`flex flex-col ${props.compact ? 'gap-1 px-3' : 'gap-2 px-4'}`}>{props.navigation.map(({ to, label, icon: Icon }) => <NavLink className={({ isActive }) => `flex min-h-11 items-center gap-3 rounded-md px-3 text-sm font-medium transition-colors ${isActive ? 'bg-accent text-accent-foreground' : 'text-muted hover:bg-default hover:text-foreground'}`} end={to === '/app/'} key={to} onClick={props.onNavigationComplete} to={to}><Icon aria-hidden className="size-5" /><span>{label}</span></NavLink>)}</nav>;
}

function ColorModeButton(props: ThemeShellProps) {
  const tr = useTranslate(); const dark = props.colorMode === 'dark';
  return <Tooltip><Tooltip.Trigger><Button aria-label={dark ? tr('Switch to light mode', '切换到浅色模式') : tr('Switch to dark mode', '切换到深色模式')} isIconOnly onPress={props.onToggleColorMode} size="sm" variant="ghost">{dark ? <Sun aria-hidden className="size-4" /> : <Moon aria-hidden className="size-4" />}</Button></Tooltip.Trigger><Tooltip.Content>{dark ? tr('Use light mode', '使用浅色模式') : tr('Use dark mode', '使用深色模式')}</Tooltip.Content></Tooltip>;
}

function AccountMenu(props: ThemeShellProps & { compact?: boolean }) {
  const tr = useTranslate();
  return <Dropdown><Dropdown.Trigger aria-label={tr(`Open account menu for ${props.userName}`, `打开 ${props.userName} 的账户菜单`)} className="inline-flex min-w-0 items-center gap-2 rounded-md p-1 hover:bg-default"><Avatar size="sm"><Avatar.Fallback>{props.userName.slice(0, 1).toUpperCase()}</Avatar.Fallback></Avatar>{!props.compact && <span className="max-w-28 truncate text-sm">{props.userName}</span>}</Dropdown.Trigger><Dropdown.Popover><Dropdown.Menu onAction={(key) => { if (key === 'profile') props.onNavigate('/app/profile'); if (key === 'logout') props.onSignOut(); }}><Dropdown.Item id="profile"><UserRound className="size-4" />{tr('Profile & stats', '个人资料与统计')}</Dropdown.Item><Dropdown.Item id="logout"><LogOut className="size-4" />{tr('Sign out', '退出登录')}</Dropdown.Item></Dropdown.Menu></Dropdown.Popover></Dropdown>;
}

export function CinemaLoginFrame(props: ThemeLoginFrameProps) {
  return <div className="cinema-login min-h-screen bg-background px-4 py-8"><main className="mx-auto grid min-h-[calc(100vh-4rem)] max-w-5xl items-stretch overflow-hidden border border-border bg-surface md:grid-cols-[minmax(0,1fr)_26rem]"><section className="cinema-login__feature hidden min-h-[34rem] flex-col justify-between p-10 md:flex"><div className="flex items-center gap-3"><img alt="" className="size-12 object-contain" height="48" src={props.logoUrl} width="48" /><span className="text-lg font-semibold">{props.siteTitle}</span></div><div><p className="max-w-md text-4xl font-semibold leading-tight">{props.siteSubtitle}</p><p className="mt-4 text-sm text-white/70">Private screenings, organized around your library.</p></div></section><section className="relative flex flex-col justify-center p-7 sm:p-10"><div className="absolute right-5 top-5">{props.actions}</div><div className="mb-8 flex items-center gap-3 md:hidden"><img alt="" className="size-11 object-contain" height="44" src={props.logoUrl} width="44" /><div><p className="font-semibold">{props.siteTitle}</p><p className="text-sm text-muted">{props.siteSubtitle}</p></div></div>{props.children}</section></main></div>;
}
