import { useState } from "react";
import { Button } from "@/components/ui/button";
import BackupPanel from "@/features/backups/BackupPanel";
import ClosedSessionHistory from "@/features/sessions/ClosedSessionHistory";
import SessionView from "@/features/sessions/SessionView";

function App() {
  const [view, setView] = useState<"session" | "history" | "backups">(
    "session",
  );

  return (
    <main className="min-h-screen px-5 py-6 sm:px-10 sm:py-8">
      <div className="mx-auto max-w-4xl">
        <header className="flex flex-wrap items-end justify-between gap-4 border-b border-border pb-5">
          <div>
            <p className="text-xs font-semibold tracking-[0.16em] text-foreground-tertiary uppercase">
              Punto de venta local
            </p>
            <h1 className="mt-1 text-2xl font-semibold tracking-[-0.04em]">
              UnaTienda
            </h1>
          </div>
          <div className="flex min-w-0 flex-1 flex-wrap items-center justify-end gap-2">
            <p className="hidden text-sm text-foreground-tertiary sm:block">
              Operación local · Sin conexión requerida
            </p>
            {view !== "session" && (
              <Button
                onClick={() => setView("session")}
                size="sm"
                type="button"
                variant="outline"
              >
                Operación
              </Button>
            )}
            {view !== "history" && (
              <Button
                onClick={() => setView("history")}
                size="sm"
                type="button"
                variant="outline"
              >
                Historial de cajas
              </Button>
            )}
            {view !== "backups" && (
              <Button
                onClick={() => setView("backups")}
                size="sm"
                type="button"
                variant="outline"
              >
                Copias de seguridad
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
        ) : view === "history" ? (
          <nav
            aria-label="Ubicación actual"
            className="border-b border-border-subtle py-4"
          >
            <p className="text-xs font-semibold tracking-[0.16em] text-primary uppercase">
              Historial de cajas
            </p>
          </nav>
        ) : (
          <nav
            aria-label="Ubicación actual"
            className="border-b border-border-subtle py-4"
          >
            <p className="text-xs font-semibold tracking-[0.16em] text-primary uppercase">
              Copias de seguridad
            </p>
          </nav>
        )}

        <div aria-hidden={view !== "session"} hidden={view !== "session"}>
          <SessionView />
        </div>
        {view === "history" && (
          <ClosedSessionHistory onBack={() => setView("session")} />
        )}
        {view === "backups" && <BackupPanel />}
      </div>
    </main>
  );
}

export default App;
