import { Fragment, useCallback, useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import {
  getClosedSessionDetail,
  listClosedSessions,
  type ClosedSessionDetail,
  type ClosedSessionSummary,
} from "../../shared/tauri/commands";
import { formatDate, formatDateTime, formatTime } from "../../shared/dateTime";
import { formatMoney } from "../../shared/formatting";
import SessionHistory from "./SessionHistory";

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

function Difference({ value, hideZero = false }: { value: string; hideZero?: boolean }) {
  if (hideZero && value === "0.00") return null;

  const details = differenceDetails(value);

  return (
    <span className={`font-semibold ${details.className}`}>
      {details.label}: {details.amount}
    </span>
  );
}

interface ClosedSessionHistoryProps {
  onBack?: () => void;
}

export default function ClosedSessionHistory(
  _props?: ClosedSessionHistoryProps,
) {
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
    <section aria-labelledby="closed-history-title" className="py-6 sm:py-8">
      <header className="border-b border-border pb-4">
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
        <div className="mt-6 grid min-h-0 gap-5 lg:grid-cols-[minmax(16rem,0.8fr)_minmax(0,1.6fr)]">
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
              className="mt-4 flex max-h-[calc(100dvh-15rem)] flex-col gap-3 overflow-y-auto overscroll-contain pr-2"
            >
              {sessions.map((session, index) => {
                const closedDate = formatDate(session.closedAt);
                const previousClosedDate =
                  index > 0 ? formatDate(sessions[index - 1].closedAt) : null;
                const startsNewDate = closedDate !== previousClosedDate;

                return (
                  <Fragment key={session.sessionId}>
                    {startsNewDate && (
                      <li className="pt-2 first:pt-0" role="presentation">
                        <div className="flex items-center gap-3 border-y border-border-subtle py-2">
                          <span
                            aria-hidden="true"
                            className="h-px flex-1 bg-border-subtle"
                          />
                          <h4 className="shrink-0 text-xs font-semibold tracking-[0.12em] text-foreground-tertiary uppercase">
                            <span className="sr-only">Corte de control: </span>
                            <time dateTime={session.closedAt}>{closedDate}</time>
                          </h4>
                          <span
                            aria-hidden="true"
                            className="h-px flex-1 bg-border-subtle"
                          />
                        </div>
                      </li>
                    )}
                    <li>
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
                        <div className="flex items-baseline justify-between gap-3">
                          <span className="font-mono text-sm font-medium tabular-nums text-foreground-secondary">
                            <time dateTime={session.openedAt}>
                              {formatTime(session.openedAt)}
                            </time>
                            <span aria-hidden="true"> – </span>
                            <time dateTime={session.closedAt}>
                              {formatTime(session.closedAt)}
                            </time>
                          </span>
                          <Difference hideZero value={session.difference} />
                        </div>
                        <div className="mt-1 flex items-baseline justify-between gap-3">
                          <span className="text-sm text-foreground-secondary">
                            Vendido
                          </span>
                          <span className="money text-lg font-semibold tabular-nums">
                            {formatMoney(session.salesTotal)}
                          </span>
                        </div>
                      </button>
                    </li>
                  </Fragment>
                );
              })}
            </ol>
          </aside>

          <div
            aria-live="polite"
            className="min-w-0 overscroll-contain lg:max-h-[calc(100dvh-13rem)] lg:overflow-y-auto lg:pr-2"
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
