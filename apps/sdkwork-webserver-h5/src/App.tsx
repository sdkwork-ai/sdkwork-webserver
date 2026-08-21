import { BrowserRouter, Route, Routes } from "react-router-dom";

export function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="*" element={<HomePage />} />
      </Routes>
    </BrowserRouter>
  );
}

function HomePage() {
  return (
    <main className="shell">
      <p className="eyebrow">SDKWork Web Server</p>
      <h1>Mobile console</h1>
      <p className="lede">
        Adaptive Web H5 surface. The standalone gateway selects this shell for
        mobile clients and falls back to the PC console when H5 is unavailable.
        Desktop clients prefer the PC console and fall back here when PC is
        missing.
      </p>
      <nav className="actions" aria-label="Console shortcuts">
        <a href="/console">Open console</a>
        <a href="/docs">Documentation</a>
        <a href="/healthz">Health</a>
      </nav>
    </main>
  );
}
