import { useCallback, useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import {
  getClosedSessionDetail,
  listClosedSessions,
  type ClosedSessionDetail,
  type ClosedSessionSummary,
} from "../../shared/tauri/commands";
import { formatDateTime } from "../../shared/dateTime";
import SessionHistory from "./SessionHistory";

function formatMoney(value: string): string {
  const negative = value.startsWith("-");
  const unsigned = negative ? value.slice(1) : value;
  const [whole, decimals = "00"] = unsigned.split(".");
  const grouped = whole.replace(/\B(?=(\d{3})+(?!\d))/g, ".");

  return `${negative ? "-" : ""}$ ${grouped},${decimals.padEnd(2, "0")}`;
}

function differenceDetails(value: string): {
  label: string;
  amount: string;
  className: string;
} {
  if (value === "0.00") {
    return {
      label: "Sin diferencia",
      amount: formatMoney(value),
      className: "text-foreground-secondary",
    };
  }

  if (value.startsWith("-")) {
    return {
      label: "Faltante",
      amount: formatMoney(value),
      className: "text-destructive",
    };
  }

  return {
    label: "Sobrante",
    amount: `+${formatMoney(value)}`,
    className: "text-primary",
  };
}

function Difference({ value }: { value: string }) {
  const details = differenceDetails(value);

  return (
    <span className={`font-semibold ${details.className}`}>
      {details.label}: {details.amount}
    </span>
  );
}

interface ClosedSessionHistoryProps {
  onBack: () => void;
}

export default function ClosedSessionHistory({
  onBack,
}: ClosedSessionHistoryProps) {
  const [sessions, setSessions] = useState<ClosedSessionSummary[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [detail, setDetail] = useState<ClosedSessionDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [detailError, setDetailError] = useState<string | null>(null);
  const [detailReloadToken, setDetailReloadToken] = useState(0);

  const loadSessions = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const nextSessions = await listClosedSessions();
      setSessions(nextSessions);
      setSelectedId((current) =>
        current && nextSessions.some((session) => session.sessionId === current)
          ? current
          : (nextSessions[0]?.sessionId ?? null),
      );
    } catch {
      setError(
        "No se pudo consultar el historial de cajas. Inténtalo de nuevo.",
      );
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadSessions();
  }, [loadSessions]);

  useEffect(() => {
    if (!selectedId) {
      setDetail(null);
      setDetailError(null);
      return;
    }

    let cancelled = false;
    setDetail(null);
    setDetailError(null);
    setDetailLoading(true);

    getClosedSessionDetail(selectedId)
      .then((nextDetail) => {
        if (!cancelled) setDetail(nextDetail);
      })
      .catch(() => {
        if (!cancelled) {
          setDetailError(
            "No se pudo consultar el detalle de la caja. Inténtalo de nuevo.",
          );
        }
      })
      .finally(() => {
        if (!cancelled) setDetailLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [detailReloadToken, selectedId]);

  return (
    <section aria-labelledby="closed-history-title" className="py-8">
      <header className="flex flex-wrap items-end justify-between gap-4 border-b border-border pb-5">
        <div>
          <p className="text-xs font-semibold tracking-[0.16em] text-primary uppercase">
            Consulta de caja
          </p>
          <h2
            id="closed-history-title"
            className="mt-2 text-3xl font-semibold tracking-tight"
          >
            Historial de cajas
          </h2>
          <p className="mt-2 max-w-xl text-foreground-secondary">
            Revisá sesiones cerradas y sus valores finales. Esta vista es de
            solo lectura.
          </p>
        </div>
        <Button onClick={onBack} type="button" variant="outline">
          Volver a la operación
        </Button>
      </header>

      {loading && (
        <p
          className="py-16 text-center text-foreground-secondary"
          role="status"
        >
          Consultando el historial de cajas…
        </p>
      )}

      {!loading && error && (
        <div className="mt-8 border border-border-strong bg-secondary p-5">
          <p className="text-sm text-destructive" role="alert">
            {error}
          </p>
          <Button className="mt-4" onClick={loadSessions} type="button">
            Reintentar consulta
          </Button>
        </div>
      )}

      {!loading && !error && sessions.length === 0 && (
        <div className="mt-8 border border-border bg-secondary p-8 text-center">
          <p className="text-lg font-semibold">Todavía no hay cajas cerradas</p>
          <p className="mx-auto mt-2 max-w-md text-sm leading-6 text-foreground-secondary">
            Cuando cierres una sesión, sus valores de apertura, operaciones y
            conciliación aparecerán acá.
          </p>
        </div>
      )}

      {!loading && !error && sessions.length > 0 && (
        <div className="mt-8 grid gap-6 lg:grid-cols-[minmax(16rem,0.8fr)_minmax(0,1.6fr)]">
          <aside aria-labelledby="closed-sessions-list-title">
            <div className="flex items-baseline justify-between gap-3 border-b border-border pb-3">
              <div>
                <p className="text-xs font-semibold tracking-[0.16em] text-foreground-tertiary uppercase">
                  Libro de cierres
                </p>
                <h3
                  id="closed-sessions-list-title"
                  className="mt-1 text-lg font-semibold"
                >
                  Sesiones cerradas
                </h3>
              </div>
              <p className="text-sm text-foreground-tertiary">
                {sessions.length}
              </p>
            </div>

            <ol
              aria-label="Sesiones cerradas"
              className="mt-4 space-y-3 lg:max-h-[calc(100dvh-20rem)] lg:overflow-y-auto lg:overscroll-contain lg:pr-2"
            >
              {sessions.map((session) => (
                <li key={session.sessionId}>
                  <button
                    aria-controls="closed-session-detail"
                    aria-current={
                      selectedId === session.sessionId ? "true" : undefined
                    }
                    className={`w-full rounded-sm border p-4 text-left transition-colors duration-150 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring ${
                      selectedId === session.sessionId
                        ? "border-border-strong bg-card"
                        : "border-border bg-secondary hover:border-border-strong hover:bg-card"
                    }`}
                    onClick={() => setSelectedId(session.sessionId)}
                    type="button"
                  >
                    <div className="flex items-start justify-between gap-3">
                      <time
                        className="font-mono text-xs tabular-nums text-foreground-tertiary"
                        dateTime={session.closedAt}
                      >
                        {formatDateTime(session.closedAt)}
                      </time>
                      <Difference value={session.difference} />
                    </div>
                    <dl className="mt-4 grid grid-cols-2 gap-x-3 gap-y-3 text-xs">
                      <div>
                        <dt className="text-foreground-tertiary">Apertura</dt>
                        <dd className="mt-1 text-foreground-secondary">
                          <time dateTime={session.openedAt}>
                            {formatDateTime(session.openedAt)}
                          </time>
                        </dd>
                      </div>
                      <div>
                        <dt className="text-foreground-tertiary">Cierre</dt>
                        <dd className="mt-1 text-foreground-secondary">
                          <time dateTime={session.closedAt}>
                            {formatDateTime(session.closedAt)}
                          </time>
                        </dd>
                      </div>
                      <div>
                        <dt className="text-foreground-tertiary">
                          Efectivo inicial
                        </dt>
                        <dd className="money mt-1">
                          {formatMoney(session.openingAmount)}
                        </dd>
                      </div>
                      <div>
                        <dt className="text-foreground-tertiary">
                          Efectivo esperado
                        </dt>
                        <dd className="money mt-1">
                          {formatMoney(session.expectedCash)}
                        </dd>
                      </div>
                      <div>
                        <dt className="text-foreground-tertiary">
                          Efectivo contado
                        </dt>
                        <dd className="money mt-1">
                          {formatMoney(session.countedCash)}
                        </dd>
                      </div>
                      <div>
                        <dt className="text-foreground-tertiary">Diferencia</dt>
                        <dd className="mt-1">
                          <Difference value={session.difference} />
                        </dd>
                      </div>
                    </dl>
                  </button>
                </li>
              ))}
            </ol>
          </aside>

          <div
            aria-live="polite"
            className="min-w-0 overscroll-contain lg:max-h-[calc(100dvh-14rem)] lg:overflow-y-auto lg:pr-2"
            id="closed-session-detail"
          >
            {detailLoading && (
              <p className="border border-border bg-secondary p-8 text-center text-foreground-secondary">
                Consultando el detalle de la caja…
              </p>
            )}

            {!detailLoading && detailError && (
              <div className="border border-border-strong bg-secondary p-5">
                <p className="text-sm text-destructive" role="alert">
                  {detailError}
                </p>
                <Button
                  className="mt-4"
                  onClick={() => setDetailReloadToken((value) => value + 1)}
                  type="button"
                >
                  Reintentar detalle
                </Button>
              </div>
            )}

            {!detailLoading && !detailError && detail && (
              <ClosedSessionDetailView detail={detail} />
            )}
          </div>
        </div>
      )}
    </section>
  );
}

function ClosedSessionDetailView({ detail }: { detail: ClosedSessionDetail }) {
  return (
    <article aria-labelledby="closed-session-detail-title">
      <header className="border-b border-border pb-3">
        <p className="text-xs font-semibold tracking-[0.16em] text-foreground-tertiary uppercase">
          Detalle consultivo
        </p>
        <h3
          id="closed-session-detail-title"
          className="mt-1 text-xl font-semibold"
        >
          Cierre del {formatDateTime(detail.closedAt)}
        </h3>
      </header>

      <dl className="mt-4 grid border-y border-border sm:grid-cols-2">
        <div className="border-b border-border-subtle p-4 sm:border-r">
          <dt className="text-sm text-foreground-tertiary">Apertura</dt>
          <dd className="mt-1 text-sm font-medium">
            <time dateTime={detail.openedAt}>
              {formatDateTime(detail.openedAt)}
            </time>
          </dd>
        </div>
        <div className="border-b border-border-subtle p-4 sm:border-b-0">
          <dt className="text-sm text-foreground-tertiary">Cierre</dt>
          <dd className="mt-1 text-sm font-medium">
            <time dateTime={detail.closedAt}>
              {formatDateTime(detail.closedAt)}
            </time>
          </dd>
        </div>
        <div className="border-b border-border-subtle p-4 sm:border-r sm:border-b-0">
          <dt className="text-sm text-foreground-tertiary">Efectivo inicial</dt>
          <dd className="money mt-1 text-xl font-semibold">
            {formatMoney(detail.openingAmount)}
          </dd>
        </div>
        <div className="border-b border-border-subtle p-4 sm:border-b-0">
          <dt className="text-sm text-foreground-tertiary">
            Efectivo esperado
          </dt>
          <dd className="money mt-1 text-xl font-semibold">
            {formatMoney(detail.expectedCash)}
          </dd>
        </div>
        <div className="border-b border-border-subtle p-4 sm:border-r sm:border-b-0">
          <dt className="text-sm text-foreground-tertiary">Efectivo contado</dt>
          <dd className="money mt-1 text-xl font-semibold">
            {formatMoney(detail.countedCash)}
          </dd>
        </div>
        <div className="p-4">
          <dt className="text-sm text-foreground-tertiary">Resultado final</dt>
          <dd className="mt-1 text-xl">
            <Difference value={detail.difference} />
          </dd>
        </div>
      </dl>

      <SessionHistory history={detail} readOnly />
    </article>
  );
}
