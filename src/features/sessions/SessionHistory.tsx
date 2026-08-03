import { useState } from "react";
import { Button } from "@/components/ui/button";
import {
  annulSale,
  type PaymentMethod,
  type SessionHistoryResponse,
  type SessionOperation,
} from "../../shared/tauri/commands";
import { formatTime } from "../../shared/dateTime";

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

const PAYMENT_METHODS: { value: PaymentMethod; label: string }[] = [
  { value: "cash", label: "Efectivo" },
  { value: "transfer", label: "Transferencia" },
  { value: "debit", label: "Débito" },
  { value: "credit", label: "Crédito" },
  { value: "qr", label: "QR" },
];

const CATEGORY_LABELS = {
  ropa: "Ropa",
  libreria: "Librería",
  mixto: "Mixto",
} as const;

function paymentMethodLabel(method: PaymentMethod): string {
  return (
    PAYMENT_METHODS.find((paymentMethod) => paymentMethod.value === method)
      ?.label ?? method
  );
}

function operationLabel(operation: SessionOperation): string {
  return operation.operationType === "sale" ? "Venta" : "Salida de caja";
}

interface SessionHistoryProps {
  history: SessionHistoryResponse;
  onAnnulled?: () => void;
  readOnly?: boolean;
}

export default function SessionHistory({
  history,
  onAnnulled,
  readOnly = false,
}: SessionHistoryProps) {
  const [pendingSale, setPendingSale] = useState<SessionOperation | null>(null);
  const [reason, setReason] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const requestAnnulment = (operation: SessionOperation) => {
    setError(null);
    setReason("");
    setPendingSale(operation);
  };

  const cancelAnnulment = () => {
    if (saving) return;
    setPendingSale(null);
    setReason("");
  };

  const confirmAnnulment = async () => {
    if (!pendingSale) return;

    try {
      setSaving(true);
      setError(null);
      await annulSale(
        {
          saleId: pendingSale.id,
          reason: reason.trim() || undefined,
        },
        generateIdempotencyKey(),
      );
      setPendingSale(null);
      setReason("");
      onAnnulled?.();
    } catch (err: unknown) {
      const message = typeof err === "string" ? err : String(err);
      setError(`No se pudo anular la venta. ${message}`);
    } finally {
      setSaving(false);
    }
  };

  return (
    <section aria-labelledby="history-title" className="mt-8">
      <div className="flex flex-wrap items-end justify-between gap-3 border-b border-border pb-3">
        <div>
          <p className="text-xs font-semibold tracking-[0.16em] text-foreground-tertiary uppercase">
            Libro de caja
          </p>
          <h3 id="history-title" className="mt-1 text-xl font-semibold">
            Historial de operaciones
          </h3>
        </div>
        <p className="text-sm text-foreground-tertiary">
          {history.operations.length}{" "}
          {history.operations.length === 1 ? "operación" : "operaciones"}
        </p>
      </div>

      <div className="mt-4 grid gap-6 lg:grid-cols-[minmax(0,1.5fr)_minmax(15rem,0.75fr)]">
        <div className="rounded-sm border border-border bg-card">
          {history.operations.length === 0 ? (
            <p className="p-5 text-sm text-foreground-secondary">
              Todavía no hay operaciones en esta sesión.
            </p>
          ) : (
            <ol
              aria-label="Operaciones de la sesión"
              className="divide-y divide-border-subtle"
            >
              {history.operations.map((operation) => {
                const isSale = operation.operationType === "sale";
                const isConfirmedSale =
                  !readOnly && isSale && operation.status === "confirmed";

                return (
                  <li className="p-4 sm:p-5" key={operation.id}>
                    <div className="flex items-start justify-between gap-4">
                      <div className="min-w-0">
                        <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                          <time
                            className="font-mono text-xs tabular-nums text-foreground-tertiary"
                            dateTime={operation.occurredAt}
                          >
                            {formatTime(operation.occurredAt)}
                          </time>
                          <span className="text-sm font-semibold">
                            {operationLabel(operation)}
                          </span>
                          {isSale && operation.status && (
                            <span
                              className={`rounded-sm px-2 py-0.5 text-xs font-semibold ${
                                operation.status === "annulled"
                                  ? "bg-destructive/10 text-destructive"
                                  : "bg-accent text-accent-foreground"
                              }`}
                            >
                              {operation.status === "annulled"
                                ? "Anulada"
                                : "Confirmada"}
                            </span>
                          )}
                        </div>
                        <p className="mt-1 text-xs text-foreground-tertiary">
                          {isSale && operation.category
                            ? `Rubro: ${CATEGORY_LABELS[operation.category]}`
                            : "Retiro de efectivo"}
                        </p>
                      </div>
                      <p className="money shrink-0 text-base font-semibold">
                        {formatMoney(operation.amount)}
                      </p>
                    </div>

                    {(operation.detail || operation.annulmentReason) && (
                      <div className="mt-3 space-y-1 border-l-2 border-border-subtle pl-3 text-sm text-foreground-secondary">
                        {operation.detail && <p>{operation.detail}</p>}
                        {operation.annulmentReason && (
                          <p>
                            <span className="font-semibold">
                              Motivo de anulación:
                            </span>{" "}
                            {operation.annulmentReason}
                          </p>
                        )}
                      </div>
                    )}

                    {isSale && operation.payments.length > 0 && (
                      <ul
                        aria-label="Desglose del pago"
                        className="mt-4 space-y-3 border-t border-border-subtle pt-3"
                      >
                        {operation.payments.map((payment, index) => (
                          <li
                            className="min-w-0 text-sm"
                            key={`${payment.method}-${index}`}
                          >
                            <p className="font-medium whitespace-normal break-normal">
                              {paymentMethodLabel(payment.method)}
                            </p>
                            {payment.recipientAccount && (
                              <p className="mt-1 break-words text-xs text-foreground-tertiary">
                                Destino: {payment.recipientAccount}
                              </p>
                            )}
                            <p className="money mt-1 whitespace-nowrap tabular-nums">
                              {formatMoney(payment.amount)}
                            </p>
                          </li>
                        ))}
                      </ul>
                    )}

                    {isConfirmedSale && (
                      <Button
                        className="mt-4"
                        disabled={saving}
                        onClick={() => requestAnnulment(operation)}
                        size="sm"
                        type="button"
                        variant="outline"
                      >
                        Anular venta
                      </Button>
                    )}
                  </li>
                );
              })}
            </ol>
          )}
        </div>

        <aside
          aria-labelledby="totals-title"
          className="h-fit rounded-sm border border-border bg-secondary p-5"
        >
          <p className="text-xs font-semibold tracking-[0.16em] text-foreground-tertiary uppercase">
            Resumen
          </p>
          <h4 id="totals-title" className="mt-1 text-lg font-semibold">
            Totales de la sesión
          </h4>
          <div className="mt-5 border-y border-border-subtle py-4">
            <p className="text-sm text-foreground-secondary">
              Efectivo esperado
            </p>
            <p className="money mt-1 text-2xl font-semibold">
              {formatMoney(history.expectedCash)}
            </p>
          </div>
          <dl className="mt-4 space-y-3">
            {PAYMENT_METHODS.map((paymentMethod) => {
              const isTransfer = paymentMethod.value === "transfer";

              return (
                <div key={paymentMethod.value}>
                  <div className="flex items-baseline justify-between gap-3 text-sm">
                    <dt className="text-foreground-secondary">
                      {paymentMethod.label}
                    </dt>
                    <dd className="money tabular-nums">
                      {formatMoney(history.paymentTotals[paymentMethod.value])}
                    </dd>
                  </div>
                  {isTransfer && history.transferTotals.length > 0 && (
                    <div
                      aria-labelledby="transfer-totals-title"
                      className="mt-2 ml-3 border-l-2 border-border-subtle pl-3"
                      role="group"
                    >
                      <p
                        className="text-xs font-semibold text-foreground-tertiary"
                        id="transfer-totals-title"
                      >
                        Transferencias por cuenta
                      </p>
                      <dl
                        aria-label="Desglose de transferencias por cuenta"
                        className="mt-2 space-y-2"
                      >
                        {history.transferTotals.map((total) => (
                          <div
                            className="flex items-baseline justify-between gap-3 text-sm"
                            key={total.recipientAccount}
                          >
                            <dt className="text-foreground-secondary">
                              {total.recipientAccount}
                            </dt>
                            <dd className="money tabular-nums">
                              {formatMoney(total.amount)}
                            </dd>
                          </div>
                        ))}
                      </dl>
                    </div>
                  )}
                </div>
              );
            })}
          </dl>
        </aside>
      </div>

      {pendingSale && (
        <div
          aria-describedby="annulment-description"
          aria-labelledby="annulment-title"
          aria-modal="true"
          className="mt-5 border border-border-strong bg-card p-5"
          role="alertdialog"
        >
          <p className="text-xs font-semibold tracking-[0.16em] text-destructive uppercase">
            Acción irreversible
          </p>
          <h4 id="annulment-title" className="mt-1 text-lg font-semibold">
            ¿Anular esta venta?
          </h4>
          <p
            id="annulment-description"
            className="mt-2 text-sm text-foreground-secondary"
          >
            La venta quedará registrada como anulada y se crearán las
            reversiones de sus medios de pago. No se borrará el comprobante.
          </p>
          <label
            className="mt-4 block text-sm font-semibold"
            htmlFor="annulment-reason"
          >
            Motivo{" "}
            <span className="font-normal text-foreground-tertiary">
              (opcional)
            </span>
          </label>
          <input
            className="mt-2 h-10 w-full rounded-sm border border-input bg-muted px-3 text-sm outline-none transition-colors duration-150 placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/35"
            disabled={saving}
            id="annulment-reason"
            onChange={(event) => setReason(event.target.value)}
            placeholder="Ej: error de carga, pedido del cliente…"
            type="text"
            value={reason}
          />
          {error && (
            <p className="mt-3 text-sm text-destructive" role="alert">
              {error}
            </p>
          )}
          <div className="mt-5 flex flex-wrap gap-3">
            <Button
              disabled={saving}
              onClick={confirmAnnulment}
              type="button"
              variant="destructive"
            >
              {saving ? "Anulando…" : "Confirmar anulación"}
            </Button>
            <Button
              disabled={saving}
              onClick={cancelAnnulment}
              type="button"
              variant="outline"
            >
              Cancelar
            </Button>
          </div>
        </div>
      )}
    </section>
  );
}
