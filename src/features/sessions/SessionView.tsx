import { useCallback, useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import {
  closeSession,
  getReconciliation,
  openSession,
  type ClosedSessionResponse,
  type ReconciliationView,
} from "../../shared/tauri/commands";

function generateIdempotencyKey(): string {
  return crypto.randomUUID();
}

function formatMoney(value: string): string {
  const negative = value.startsWith("-");
  const unsigned = negative ? value.slice(1) : value;
  const [whole, decimals = "00"] = unsigned.split(".");
  const grouped = whole.replace(/\B(?=(\d{3})+(?!\d))/g, ".");

  return `${negative ? "-" : ""}$ ${grouped},${decimals.padEnd(2, "0")}`;
}

function errorMessage(action: "abrir" | "cerrar"): string {
  return `No se pudo ${action} la caja. Revisa la conexión e inténtalo de nuevo.`;
}

const inputClassName =
  "h-12 w-full rounded-sm border border-input bg-muted px-3 font-mono text-lg tabular-nums outline-none transition-colors duration-150 placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/35 disabled:cursor-not-allowed disabled:opacity-50";

export default function SessionView() {
  const [session, setSession] = useState<ReconciliationView | null>(null);
  const [openingAmount, setOpeningAmount] = useState("");
  const [countedCash, setCountedCash] = useState("");
  const [closedResult, setClosedResult] =
    useState<ClosedSessionResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [opening, setOpening] = useState(false);
  const [closing, setClosing] = useState(false);
  const [confirmingClose, setConfirmingClose] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const openKeyRef = useRef<string | null>(null);
  const closeKeyRef = useRef<string | null>(null);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const loadSession = useCallback(async (counted?: string) => {
    try {
      setLoading(true);
      setError(null);
      setSession(await getReconciliation(counted));
    } catch (err: unknown) {
      const message = typeof err === "string" ? err : String(err);
      if (
        message.includes("NO_OPEN_SESSION") ||
        message.includes("no open session")
      ) {
        setSession(null);
      } else {
        setError("No se pudo consultar la sesión de caja. Inténtalo de nuevo.");
      }
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadSession();
  }, [loadSession]);

  useEffect(() => {
    if (debounceRef.current) clearTimeout(debounceRef.current);

    if (!countedCash.trim()) {
      setSession((current) =>
        current
          ? { ...current, countedCash: undefined, difference: undefined }
          : current,
      );
      setConfirmingClose(false);
      return;
    }

    debounceRef.current = setTimeout(() => {
      loadSession(countedCash.trim());
    }, 300);

    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, [countedCash, loadSession]);

  const handleOpen = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!openingAmount.trim()) return;
    if (!openKeyRef.current) openKeyRef.current = generateIdempotencyKey();

    try {
      setOpening(true);
      setError(null);
      await openSession(openingAmount.trim(), openKeyRef.current);
      await loadSession();
      openKeyRef.current = null;
    } catch {
      setError(errorMessage("abrir"));
    } finally {
      setOpening(false);
    }
  };

  const handleClose = async () => {
    if (!session?.difference || !countedCash.trim()) return;
    if (!closeKeyRef.current) closeKeyRef.current = generateIdempotencyKey();

    try {
      setClosing(true);
      setError(null);
      const result = await closeSession(
        countedCash.trim(),
        closeKeyRef.current,
      );
      setClosedResult(result);
      setSession(null);
      closeKeyRef.current = null;
    } catch {
      setError(errorMessage("cerrar"));
    } finally {
      setClosing(false);
    }
  };

  if (loading && !session) {
    return (
      <p className="py-16 text-center text-foreground-secondary" role="status">
        Consultando la sesión de caja…
      </p>
    );
  }

  if (closedResult) {
    return (
      <section aria-labelledby="closed-title" className="py-8">
        <p className="text-xs font-semibold tracking-[0.16em] text-primary uppercase">
          Jornada completada
        </p>
        <h2
          id="closed-title"
          className="mt-2 text-3xl font-semibold tracking-tight"
        >
          Caja cerrada
        </h2>
        <p className="mt-2 text-foreground-secondary">
          La conciliación quedó registrada y ya puedes iniciar una nueva sesión.
        </p>
        <dl className="mt-8 grid border-y border-border sm:grid-cols-2">
          {[
            ["Efectivo inicial", closedResult.openingAmount],
            ["Efectivo esperado", closedResult.expectedCash],
            ["Efectivo contado", closedResult.countedCash],
            ["Diferencia", closedResult.difference],
          ].map(([label, value]) => (
            <div
              className="border-b border-border-subtle p-4 last:border-b-0 sm:[&:nth-last-child(-n+2)]:border-b-0"
              key={label}
            >
              <dt className="text-sm text-foreground-tertiary">{label}</dt>
              <dd className="money mt-1 text-xl font-semibold">
                {formatMoney(value)}
              </dd>
            </div>
          ))}
        </dl>
        <Button
          className="mt-6"
          onClick={() => {
            setClosedResult(null);
            setOpeningAmount("");
            setCountedCash("");
            setConfirmingClose(false);
            setError(null);
          }}
          size="lg"
        >
          Abrir una nueva caja
        </Button>
      </section>
    );
  }

  if (!session) {
    return (
      <section aria-labelledby="open-title" className="py-8">
        <p className="text-xs font-semibold tracking-[0.16em] text-primary uppercase">
          Paso 1 de 3 · Apertura
        </p>
        <h2
          id="open-title"
          className="mt-2 text-3xl font-semibold tracking-tight"
        >
          Abrir sesión de caja
        </h2>
        <p className="mt-2 max-w-xl leading-7 text-foreground-secondary">
          Cuenta el fondo disponible antes de la primera venta. Este importe
          será el punto de partida de la jornada.
        </p>

        <form className="mt-8 max-w-sm" onSubmit={handleOpen}>
          <label className="text-sm font-semibold" htmlFor="opening-amount">
            Efectivo inicial
          </label>
          <div className="relative mt-2">
            <span className="pointer-events-none absolute inset-y-0 left-3 flex items-center font-mono text-foreground-tertiary">
              $
            </span>
            <input
              aria-invalid={Boolean(error)}
              className={`${inputClassName} pl-8`}
              disabled={opening}
              id="opening-amount"
              inputMode="decimal"
              onChange={(event) => setOpeningAmount(event.target.value)}
              placeholder="0.00"
              type="text"
              value={openingAmount}
            />
          </div>
          {error && (
            <p className="mt-3 text-sm text-destructive" role="alert">
              {error}
            </p>
          )}
          <Button
            className="mt-6 w-full"
            disabled={opening || !openingAmount.trim()}
            size="lg"
            type="submit"
          >
            {opening
              ? "Abriendo caja…"
              : error
                ? "Reintentar apertura"
                : "Abrir caja"}
          </Button>
        </form>
      </section>
    );
  }

  const difference = session.difference;
  const differenceValue = difference ? formatMoney(difference) : null;
  const differenceLabel =
    difference === "0.00"
      ? "Sin diferencia"
      : difference?.startsWith("-")
        ? differenceValue
        : differenceValue
          ? `+${differenceValue}`
          : "Pendiente de conteo";
  const confirmationText =
    difference === "0.00"
      ? "El efectivo contado coincide. La sesión quedará cerrada."
      : `Se registrará un ${difference?.startsWith("-") ? "faltante" : "sobrante"} de ${differenceValue?.replace("-", "")}.`;

  return (
    <section aria-labelledby="reconcile-title" className="py-8">
      <p className="text-xs font-semibold tracking-[0.16em] text-primary uppercase">
        Paso 2 de 3 · Conciliación
      </p>
      <h2
        id="reconcile-title"
        className="mt-2 text-3xl font-semibold tracking-tight"
      >
        Conciliar caja
      </h2>
      <p className="mt-2 text-foreground-secondary">
        Cuenta el efectivo del cajón. La diferencia se calcula de forma exacta
        antes de confirmar el cierre.
      </p>

      <dl className="mt-8 grid border-y border-border sm:grid-cols-2">
        <div className="p-4 sm:border-r sm:border-border-subtle">
          <dt className="text-sm text-foreground-tertiary">Efectivo inicial</dt>
          <dd className="money mt-1 text-2xl font-semibold">
            {formatMoney(session.openingAmount)}
          </dd>
        </div>
        <div className="border-t border-border-subtle p-4 sm:border-t-0">
          <dt className="text-sm text-foreground-tertiary">
            Efectivo esperado
          </dt>
          <dd className="money mt-1 text-2xl font-semibold">
            {formatMoney(session.expectedCash)}
          </dd>
        </div>
      </dl>

      <div className="mt-8 grid gap-6 sm:grid-cols-[1fr_1fr] sm:items-end">
        <div>
          <label className="text-sm font-semibold" htmlFor="counted-cash">
            Efectivo contado
          </label>
          <input
            aria-invalid={Boolean(error)}
            className={`${inputClassName} mt-2`}
            disabled={closing}
            id="counted-cash"
            inputMode="decimal"
            onChange={(event) => {
              setCountedCash(event.target.value);
              setConfirmingClose(false);
            }}
            placeholder="0.00"
            type="text"
            value={countedCash}
          />
        </div>
        <div
          aria-live="polite"
          className="border-l-2 border-border-strong pl-4"
        >
          <p className="text-sm text-foreground-tertiary">Diferencia</p>
          <p
            className={`money mt-1 text-xl font-semibold ${difference && difference !== "0.00" ? "text-destructive" : ""}`}
          >
            {differenceLabel}
          </p>
        </div>
      </div>

      {confirmingClose && difference && (
        <div
          className="mt-6 border border-border-strong bg-secondary p-4"
          role="status"
        >
          <p className="font-semibold">Revisa antes de cerrar</p>
          <p className="mt-1 text-sm text-foreground-secondary">
            {confirmationText}
          </p>
        </div>
      )}
      {error && (
        <p className="mt-4 text-sm text-destructive" role="alert">
          {error}
        </p>
      )}

      <div className="mt-6 flex flex-wrap gap-3">
        {!confirmingClose ? (
          <Button
            disabled={!difference || closing}
            onClick={() => setConfirmingClose(true)}
            size="lg"
          >
            Revisar cierre
          </Button>
        ) : (
          <>
            <Button
              disabled={closing}
              onClick={handleClose}
              size="lg"
              variant="destructive"
            >
              {closing
                ? "Cerrando caja…"
                : error
                  ? "Reintentar cierre"
                  : "Confirmar cierre"}
            </Button>
            <Button
              disabled={closing}
              onClick={() => setConfirmingClose(false)}
              size="lg"
              variant="outline"
            >
              Volver al conteo
            </Button>
          </>
        )}
      </div>
    </section>
  );
}
