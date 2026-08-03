import { useRef, useState } from "react";
import { cn } from "@/lib/utils";
import BackupPanel from "@/features/backups/BackupPanel";
import ClosedSessionHistory from "@/features/sessions/ClosedSessionHistory";
import SessionView, { type SessionStep } from "@/features/sessions/SessionView";

type View = "session" | "history" | "backups";

const navigationItems: { value: View; label: string }[] = [
  { value: "session", label: "Operación" },
  { value: "history", label: "Historial" },
  { value: "backups", label: "Copias" },
];

const sessionSteps: { value: SessionStep; label: string }[] = [
  { value: "opening", label: "Apertura" },
  { value: "operation", label: "Operación" },
  { value: "reconcile", label: "Cierre" },
];

function App() {
  const [view, setView] = useState<View>("session");
  const [sessionStep, setSessionStep] = useState<SessionStep>("opening");
  const mainRef = useRef<HTMLElement>(null);
  const displayedStep = sessionStep === "closed" ? "reconcile" : sessionStep;
  const activeStepIndex = sessionSteps.findIndex(
    (step) => step.value === displayedStep,
  );

  const handleNavigation = (nextView: View) => {
    if (nextView === view) return;
    setView(nextView);
    mainRef.current?.focus();
  };

  return (
    <div className="min-h-dvh bg-background">
      <a
        className="sr-only focus:not-sr-only focus:fixed focus:top-4 focus:left-4 focus:z-10 focus:rounded-sm focus:bg-primary focus:px-4 focus:py-3 focus:text-sm focus:font-semibold focus:text-primary-foreground"
        href="#main-content"
      >
        Saltar al contenido
      </a>

      <aside
        aria-label="Navegación principal"
        className="fixed inset-y-0 left-0 flex w-56 flex-col border-r border-border bg-background"
      >
        <div className="border-b border-border-subtle px-5 py-6">
          <p className="text-xs font-semibold tracking-[0.16em] text-foreground-tertiary uppercase">
            Punto de venta local
          </p>
          <h1 className="mt-2 text-2xl font-semibold tracking-[-0.04em]">
            UnaTienda
          </h1>
        </div>

        <nav aria-label="Secciones" className="flex flex-1 flex-col gap-1 p-4">
          {navigationItems.map((item) => {
            const isActive = view === item.value;

            return (
              <button
                aria-current={isActive ? "page" : undefined}
                className={cn(
                  "min-h-11 rounded-sm border-l-2 px-3 text-left text-sm transition-colors duration-150 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring",
                  isActive
                    ? "border-primary bg-secondary font-semibold text-foreground"
                    : "border-transparent text-foreground-secondary hover:border-border-strong hover:bg-secondary/60 hover:text-foreground",
                )}
                key={item.value}
                onClick={() => handleNavigation(item.value)}
                type="button"
              >
                {item.label}
              </button>
            );
          })}
        </nav>

        <div className="border-t border-border-subtle px-5 py-5">
          {view === "session" && (
            <>
              <p className="text-xs font-semibold tracking-[0.14em] text-foreground-tertiary uppercase">
                Progreso de sesión
              </p>
              <ol
                aria-label="Progreso de la sesión"
                className="mt-4 flex flex-col gap-3"
              >
                {sessionSteps.map((step, index) => {
                  const isCurrent = step.value === displayedStep;
                  const isComplete =
                    activeStepIndex >= 0 && index < activeStepIndex;

                  return (
                    <li
                      aria-current={isCurrent ? "step" : undefined}
                      className={cn(
                        "flex items-center gap-2 text-sm text-foreground-tertiary",
                        isCurrent && "font-semibold text-foreground",
                        isComplete && "text-primary",
                      )}
                      key={step.value}
                    >
                      <span
                        aria-hidden="true"
                        className={cn(
                          "size-2 rounded-full border",
                          isCurrent && "border-primary bg-primary",
                          isComplete && "border-primary",
                          !isCurrent && !isComplete && "border-border-strong",
                        )}
                      />
                      {step.label}
                    </li>
                  );
                })}
              </ol>
            </>
          )}
          <p
            className={cn(
              "text-xs leading-5 text-foreground-tertiary",
              view === "session" && "mt-5",
            )}
          >
            Operación local · Sin conexión requerida
          </p>
        </div>
      </aside>

      <main
        className="min-h-dvh min-w-0 pl-56"
        id="main-content"
        ref={mainRef}
        tabIndex={-1}
      >
        <div className="mx-auto w-full max-w-[90rem] px-6 py-6 sm:px-8 sm:py-8 xl:px-10 2xl:px-12">
          <div aria-hidden={view !== "session"} hidden={view !== "session"}>
            <SessionView onProgressChange={setSessionStep} />
          </div>
          {view === "history" && <ClosedSessionHistory />}
          {view === "backups" && <BackupPanel />}
        </div>
      </main>
    </div>
  );
}

export default App;
