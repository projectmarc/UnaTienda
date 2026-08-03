import { useState } from "react";
import { Button } from "@/components/ui/button";
import ClosedSessionHistory from "@/features/sessions/ClosedSessionHistory";
import SessionView from "@/features/sessions/SessionView";

function App() {
  const [view, setView] = useState<"session" | "history">("session");

  return (
    <main className="min-h-screen px-5 py-6 sm:px-10 sm:py-8">
      <div className="mx-auto max-w-4xl">
        <header className="flex items-end justify-between gap-6 border-b border-border pb-5">
          <div>
            <p className="text-xs font-semibold tracking-[0.16em] text-foreground-tertiary uppercase">
              Punto de venta local
            </p>
            <h1 className="mt-1 text-2xl font-semibold tracking-[-0.04em]">
              UnaTienda
            </h1>
          </div>
          <div className="flex items-center gap-4">
            <p className="hidden text-sm text-foreground-tertiary sm:block">
              Operación local · Sin conexión requerida
            </p>
            {view === "session" && (
              <Button
                onClick={() => setView("history")}
                size="sm"
                type="button"
                variant="outline"
              >
                Historial de cajas
              </Button>
            )}
          </div>
        </header>

        {view === "session" ? (
          <nav
            aria-label="Progreso de la sesión"
            className="border-b border-border-subtle py-4"
          >
            <ol className="grid grid-cols-3 gap-2 text-xs font-semibold text-foreground-tertiary sm:text-sm">
              <li>Apertura</li>
              <li className="text-center">Operación</li>
              <li className="text-right">Cierre</li>
            </ol>
          </nav>
        ) : (
          <nav
            aria-label="Ubicación actual"
            className="border-b border-border-subtle py-4"
          >
            <p className="text-xs font-semibold tracking-[0.16em] text-primary uppercase">
              Historial de cajas
            </p>
          </nav>
        )}

        <div aria-hidden={view === "history"} hidden={view === "history"}>
          <SessionView />
        </div>
        {view === "history" && (
          <ClosedSessionHistory onBack={() => setView("session")} />
        )}
      </div>
    </main>
  );
}

export default App;
