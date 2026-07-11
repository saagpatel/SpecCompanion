import type { ReactNode } from "react";
import { NavLink } from "react-router-dom";

const navItems = [
  { to: "/", label: "Dashboard", icon: "grid" },
  { to: "/settings", label: "Settings", icon: "cog" },
];

const icons: Record<string, ReactNode> = {
  grid: (
    <svg className="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth={2}
        d="M4 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2V6zm10 0a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2V6zM4 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2v-2zm10 0a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2v-2z"
      />
    </svg>
  ),
  cog: (
    <svg className="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth={2}
        d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"
      />
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth={2}
        d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"
      />
    </svg>
  ),
};

export function Sidebar() {
  return (
    <aside className="bg-surface-alt border-border flex h-auto w-full shrink-0 items-center border-b md:h-screen md:w-60 md:flex-col md:items-stretch md:border-r md:border-b-0">
      <div className="border-border shrink-0 px-3 py-2 md:border-b md:p-4">
        <h1 className="text-primary-light text-base font-bold tracking-tight md:text-lg">
          Spec Companion
        </h1>
        <p className="text-text-muted mt-0.5 hidden text-xs md:block">
          Spec &rarr; Tests &rarr; Alignment
        </p>
      </div>
      <nav
        aria-label="Primary"
        className="flex flex-1 justify-end gap-1 p-2 md:flex-col md:justify-start md:space-y-1 md:p-3"
      >
        {navItems.map((item) => (
          <NavLink
            key={item.to}
            to={item.to}
            end={item.to === "/"}
            className={({ isActive }) =>
              `flex items-center gap-2 rounded-lg px-2 py-2 text-sm transition-colors md:gap-3 md:px-3 ${
                isActive
                  ? "bg-primary/15 text-primary-light font-medium"
                  : "text-text-muted hover:bg-surface-hover hover:text-text"
              }`
            }
          >
            {icons[item.icon]}
            {item.label}
          </NavLink>
        ))}
      </nav>
      <div className="border-border text-text-muted mt-auto hidden border-t p-4 text-xs md:block">
        v0.1.0
      </div>
    </aside>
  );
}
