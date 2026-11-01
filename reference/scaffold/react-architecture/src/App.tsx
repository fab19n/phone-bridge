/**
 * App — root component with routing.
 *
 * Routes:
 *   /              → Dashboard (device status, paired devices)
 *   /mirror/:serial → Mirror session (canvas + controls)
 *   /settings      → Settings page (Phase 2)
 */

import { BrowserRouter, Routes, Route } from 'react-router-dom';
import { DashboardPage } from './pages/Dashboard';
import { MirrorPage } from './pages/Mirror';
// import { SettingsPage } from './pages/Settings'; // Phase 2

export default function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<DashboardPage />} />
        <Route path="/mirror/:serial" element={<MirrorPage />} />
        {/* <Route path="/settings" element={<SettingsPage />} /> */}
      </Routes>
    </BrowserRouter>
  );
}
