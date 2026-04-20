// App shell for the multi-project dashboard (GH #429).
// Wires the QueryClient + BrowserRouter and hands off to the
// top-level routes. Real SCADA layout (sidebar, global alert rail,
// terminal list) lands in later P1.* subissues.

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { BrowserRouter, Route, Routes } from "react-router-dom";

import { ProjectDetail } from "@/pages/ProjectDetail";
import { ProjectGrid } from "@/pages/ProjectGrid";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      // Errors bubble to components; no auto-retry on 4xx noise.
      retry: false,
      refetchOnWindowFocus: false,
    },
  },
});

export function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <div className="min-h-screen bg-background text-foreground">
          <Routes>
            <Route path="/" element={<ProjectGrid />} />
            <Route path="/project/*" element={<ProjectDetail />} />
          </Routes>
        </div>
      </BrowserRouter>
    </QueryClientProvider>
  );
}
