import { useEffect } from 'react';
import type { ThemeMode } from '../types/generated';

export function resolveEffectiveTheme(mode: ThemeMode): 'dark' | 'light' {
  if (mode === 'light') {
    return 'light';
  }
  if (mode === 'dark') {
    return 'dark';
  }
  if (typeof window !== 'undefined' && typeof window.matchMedia === 'function') {
    return window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark';
  }
  return 'dark';
}

export function useTheme(mode: ThemeMode): void {
  useEffect(() => {
    const apply = () => {
      const effective = resolveEffectiveTheme(mode);
      document.documentElement.dataset.theme = effective;
      document.documentElement.classList.toggle('dark', effective === 'dark');
    };

    apply();

    if (
      mode !== 'system' ||
      typeof window === 'undefined' ||
      typeof window.matchMedia !== 'function'
    ) {
      return undefined;
    }

    const media = window.matchMedia('(prefers-color-scheme: light)');
    const listener = () => apply();
    media.addEventListener('change', listener);
    return () => {
      media.removeEventListener('change', listener);
    };
  }, [mode]);
}
