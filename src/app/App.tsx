import { useCallback, useRef, useState } from "react";
import { cn } from "@/lib/utils";
import BackupPanel from "@/features/backups/BackupPanel";
import ClosedSessionHistory from "@/features/sessions/ClosedSessionHistory";
import SessionView, { type SessionStep } from "@/features/sessions/SessionView";

type View =
  | "sales"
  | "current-cash"
  | "history"
  | "movements"
  | "configuration"
  | "backups";

type NavigationItem = { value: View; label: string; disabled?: boolean };

const primaryNavigation: NavigationItem[] = [
  { value: "sales", label: "Ventas" },
  { value: "current-cash", label: "Caja actual" },
  { value: "history", label: "Historial" },
];

const secondaryNavigation: NavigationItem[] = [
  { value: "movements", label: "Movimientos" },
  { value: "configuration", label: "Configuración", disabled: true },
];

const sessionSteps: { value: SessionStep; label: string }[] = [
  { value: "opening", label: "Apertura" },
  { value: "operation", label: "Operación" },
  { value: "reconcile", label: "Cierre" },
];

function TransitionalView({
  description,
  title,
}: {
  description: string;
  title: string;
}) {
  return (
    <section
      aria-disabled="true"
      aria-labelledby={`${title}-title`}
      className="py-8"
    >
      <p className="text-xs font-semibold tracking-[0.16em] text-foreground-tertiary uppercase">
        Próxima actualización
      </p>
      <h2
        id={`${title}-title`}
        className="mt-2 text-3xl font-semibold tracking-tight"
      >
        {title}
      </h2>
      <p className="mt-3 max-w-xl leading-7 text-foreground-secondary">
        Disponible en la siguiente actualización.
      </p>
      <p className="mt-2 max-w-xl text-sm text-foreground-tertiary">
        {description}
      </p>
    </section>
  );
}

function App() {
  const [view, setView] = useState<View>("sales");
  const [sessionStep, setSessionStep] = useState<SessionStep>("opening");
  const [requestedSessionStep, setRequestedSessionStep] = useState<
    "reconcile" | null
  >(null);
  const mainRef = useRef<HTMLElement>(null);
  const displayedStep = sessionStep === "closed" ? "reconcile" : sessionStep;
  const activeStepIndex = sessionSteps.findIndex(
    (step) => step.value === displayedStep,
  );

  const handleProgressChange = useCallback((nextStep: SessionStep) => {
    setSessionStep(nextStep);
    if (nextStep !== "reconcile") setRequestedSessionStep(null);
  }, []);

  const handleNavigation = (nextView: View) => {
    if (nextView === view) return;
    setView(nextView);
    mainRef.current?.focus();
  };

  const handleCloseStep = () => {
    setRequestedSessionStep("reconcile");
    setView("sales");
    mainRef.current?.focus();
  };

  const renderNavigationItem = (item: NavigationItem) => {
    const isActive = view === item.value;

    return (
      <button
        aria-current={isActive ? "page" : undefined}
        aria-disabled={item.disabled || undefined}
        className={cn(
          "min-h-11 rounded-sm border-l-2 px-3 text-left text-sm transition-colors duration-150 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring disabled:cursor-not-allowed disabled:opacity-50",
          isActive
            ? "border-primary bg-secondary font-semibold text-foreground"
            : "border-transparent text-foreground-secondary hover:border-border-strong hover:bg-secondary/60 hover:text-foreground",
        )}
        disabled={item.disabled}
        key={item.value}
        onClick={() => handleNavigation(item.value)}
        type="button"
      >
        {item.label}
      </button>
    );
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
        className="border-b border-border bg-background md:fixed md:inset-y-0 md:left-0 md:flex md:w-56 md:flex-col md:border-r md:border-b-0"
      >
        <div className="border-b border-border-subtle px-5 py-6">
          <p className="text-xs font-semibold tracking-[0.16em] text-foreground-tertiary uppercase">
            Punto de venta local
          </p>
          <h1 className="mt-2 text-2xl font-semibold tracking-[-0.04em]">
            UnaTienda
          </h1>
        </div>

        <nav
          aria-label="Secciones"
          className="flex flex-col gap-1 p-4 md:flex-1"
        >
          <div aria-labelledby="primary-navigation-title" role="group">
            <p
              className="text-xs font-semibold tracking-[0.14em] text-foreground-tertiary uppercase"
              id="primary-navigation-title"
            >
              Principal
            </p>
            <div className="mt-2 flex flex-col gap-1">
              {primaryNavigation.map(renderNavigationItem)}
            </div>
          </div>
          <div
            aria-labelledby="secondary-navigation-title"
            className="mt-4 border-t border-border-subtle pt-4"
            role="group"
          >
            <p
              className="text-xs font-semibold tracking-[0.14em] text-foreground-tertiary uppercase"
              id="secondary-navigation-title"
            >
              Secundario
            </p>
            <div className="mt-2 flex flex-col gap-1">
              {secondaryNavigation.map(renderNavigationItem)}
            </div>
          </div>
        </nav>

        <nav
          aria-label="Herramientas existentes"
          className="border-t border-border-subtle px-4 py-4"
        >
          <p className="px-3 text-xs font-semibold tracking-[0.14em] text-foreground-tertiary uppercase">
            Herramientas existentes
          </p>
          <div className="mt-2 flex flex-col gap-1">
            {renderNavigationItem({
              value: "backups",
              label: "Copias de seguridad",
            })}
          </div>
        </nav>

        <div className="border-t border-border-subtle px-5 py-5">
          <p className="text-xs font-semibold tracking-[0.14em] text-foreground-tertiary uppercase">
            Progreso de sesión
          </p>
          <ol
            aria-label="Progreso de la sesión"
            className="mt-4 flex flex-col gap-1"
          >
            {sessionSteps.map((step, index) => {
              const isCurrent = step.value === displayedStep;
              const isComplete =
                activeStepIndex >= 0 && index < activeStepIndex;
              const isCloseStep = step.value === "reconcile";
              const canReachClose =
                sessionStep !== "opening" && sessionStep !== "closed";

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
                      "size-2 shrink-0 rounded-full border",
                      isCurrent && "border-primary bg-primary",
                      isComplete && "border-primary",
                      !isCurrent && !isComplete && "border-border-strong",
                    )}
                  />
                  {isCloseStep ? (
                    <button
                      className="min-h-11 flex-1 text-left disabled:cursor-not-allowed disabled:opacity-50"
                      disabled={!canReachClose}
                      onClick={handleCloseStep}
                      type="button"
                    >
                      {step.label}
                    </button>
                  ) : (
                    <span className="flex min-h-11 items-center">
                      {step.label}
                    </span>
                  )}
                </li>
              );
            })}
          </ol>
          <p className="mt-5 text-xs leading-5 text-foreground-tertiary">
            Modo local · Los datos se guardan en este equipo
          </p>
        </div>
      </aside>

      <main
        className="min-h-dvh min-w-0 md:pl-56"
        id="main-content"
        ref={mainRef}
        tabIndex={-1}
      >
        <div className="mx-auto w-full max-w-[90rem] px-6 py-6 sm:px-8 sm:py-8 xl:px-10 2xl:px-12">
          <div aria-hidden={view !== "sales"} hidden={view !== "sales"}>
            <SessionView
              onProgressChange={handleProgressChange}
              requestedStep={requestedSessionStep}
            />
          </div>
          {view === "history" && <ClosedSessionHistory />}
          {view === "current-cash" && (
            <TransitionalView
              description="Los datos de la caja siguen disponibles en Ventas mientras se completa esta sección."
              title="Caja actual"
            />
          )}
          {view === "movements" && (
            <TransitionalView
              description="La salida de caja sigue disponible dentro del flujo actual de Ventas."
              title="Movimientos"
            />
          )}
          {view === "backups" && <BackupPanel />}
        </div>
      </main>
    </div>
  );
}

export default App;
