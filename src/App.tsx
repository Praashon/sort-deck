import { MemoryRouter, Route, Routes } from 'react-router';
import { AppShell } from './components/layout/AppShell';
import { AppDataProvider } from './hooks/useAppData';
import { ToastProvider } from './hooks/useToast';
import { Dashboard } from './pages/Dashboard';
import { Folders } from './pages/Folders';
import { History } from './pages/History';
import { Recovery } from './pages/Recovery';
import { Rules } from './pages/Rules';
import { Settings } from './pages/Settings';

interface AppProps {
  initialRoute?: string | undefined;
  forceIpc?: boolean | undefined;
}

export function App({ initialRoute = '/', forceIpc }: AppProps) {
  return (
    <MemoryRouter initialEntries={[initialRoute]}>
      <ToastProvider>
        <AppDataProvider forceIpc={forceIpc}>
          <AppShell>
            <Routes>
              <Route path="/" element={<Dashboard />} />
              <Route path="/folders" element={<Folders />} />
              <Route path="/rules" element={<Rules />} />
              <Route path="/history" element={<History />} />
              <Route path="/recovery" element={<Recovery />} />
              <Route path="/settings" element={<Settings />} />
              <Route path="*" element={<Dashboard />} />
            </Routes>
          </AppShell>
        </AppDataProvider>
      </ToastProvider>
    </MemoryRouter>
  );
}

export default App;
