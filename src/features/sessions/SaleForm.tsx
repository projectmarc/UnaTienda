import { useState } from "react";
import { Button } from "@/components/ui/button";
import {
  recordSale,
  type PaymentMethod,
  type SaleCategory,
  type PaymentPartRequest,
} from "../../shared/tauri/commands";

function generateIdempotencyKey(): string {
  return crypto.randomUUID();
}

const CATEGORIES: { value: SaleCategory; label: string }[] = [
  { value: "ropa", label: "Ropa" },
  { value: "libreria", label: "Librería" },
  { value: "mixto", label: "Mixto" },
];

const PAYMENT_METHODS: { value: PaymentMethod; label: string }[] = [
  { value: "cash", label: "Efectivo" },
  { value: "transfer", label: "Transferencia" },
  { value: "debit", label: "Débito" },
  { value: "credit", label: "Crédito" },
  { value: "qr", label: "QR" },
];

const RECIPIENT_ACCOUNTS = ["Juan", "Carlos"];

function paymentMethodLabel(method: PaymentMethod): string {
  return (
    PAYMENT_METHODS.find((paymentMethod) => paymentMethod.value === method)
      ?.label ?? method
  );
}

const inputClassName =
  "h-12 w-full rounded-sm border border-input bg-muted px-3 font-mono text-lg tabular-nums outline-none transition-colors duration-150 placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/35 disabled:cursor-not-allowed disabled:opacity-50";

interface SaleFormProps {
  onRecorded: () => void;
}

export default function SaleForm({ onRecorded }: SaleFormProps) {
  const [totalAmount, setTotalAmount] = useState("");
  const [category, setCategory] = useState<SaleCategory | "">("");
  const [detail, setDetail] = useState("");
  const [method, setMethod] = useState<PaymentMethod | "">("");
  const [recipientAccount, setRecipientAccount] = useState("");
  const [additionalPayments, setAdditionalPayments] = useState<
    PaymentPartRequest[]
  >([]);
  const [showAddForm, setShowAddForm] = useState(false);
  const [addMethod, setAddMethod] = useState<PaymentMethod | "">("");
  const [addAmount, setAddAmount] = useState("");
  const [addRecipient, setAddRecipient] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);

  const resetForm = () => {
    setTotalAmount("");
    setCategory("");
    setDetail("");
    setMethod("");
    setRecipientAccount("");
    setAdditionalPayments([]);
    setShowAddForm(false);
    setAddMethod("");
    setAddAmount("");
    setAddRecipient("");
    setError(null);
  };

  const addAdditionalPayment = () => {
    if (!addMethod || !addAmount.trim()) return;

    const parsed = parseFloat(addAmount.trim());
    if (isNaN(parsed) || parsed <= 0) return;

    const remainingAfterAdd = expectedTotal - additionalTotal - parsed;
    if (remainingAfterAdd <= 0) return;

    const part: PaymentPartRequest = {
      method: addMethod,
      amount: addAmount.trim(),
      recipientAccount:
        addMethod === "transfer" ? addRecipient || undefined : undefined,
    };

    setAdditionalPayments((prev) => [...prev, part]);
    setAddMethod("");
    setAddAmount("");
    setAddRecipient("");
  };

  const removeAdditionalPayment = (index: number) => {
    setAdditionalPayments((prev) => prev.filter((_, i) => i !== index));
  };

  const additionalTotal = additionalPayments.reduce(
    (sum, p) => sum + parseFloat(p.amount || "0"),
    0,
  );
  const expectedTotal = parseFloat(totalAmount || "0");
  const isSplitMode = showAddForm || additionalPayments.length > 0;
  const parsedAddAmount = parseFloat(addAmount.trim() || "0");
  const pendingAdditionalAmount =
    isSplitMode &&
    addMethod &&
    Number.isFinite(parsedAddAmount) &&
    parsedAddAmount > 0
      ? parsedAddAmount
      : 0;
  const additionalTotalWithPending = additionalTotal + pendingAdditionalAmount;
  const computedPrimaryAmount = expectedTotal - additionalTotal;
  const displayedPrimaryAmount = expectedTotal - additionalTotalWithPending;
  const primaryAmountValid = !isSplitMode || computedPrimaryAmount > 0;
  const displayedPrimaryAmountValid =
    !isSplitMode || displayedPrimaryAmount > 0;
  const pendingAdditionalAmountInvalid =
    Boolean(isSplitMode && addMethod && addAmount.trim()) &&
    (!Number.isFinite(parsedAddAmount) || parsedAddAmount <= 0);
  const assignedTotal =
    additionalTotalWithPending + Math.max(displayedPrimaryAmount, 0);
  const splitPaymentValid =
    displayedPrimaryAmountValid &&
    !pendingAdditionalAmountInvalid &&
    Number.isFinite(assignedTotal) &&
    Number.isFinite(expectedTotal) &&
    Math.abs(assignedTotal - expectedTotal) < 0.005;

  const handleSubmit = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!totalAmount.trim() || !category) return;
    if (!method) return;

    let payments: PaymentPartRequest[];

    if (isSplitMode) {
      if (!primaryAmountValid) {
        setError("Los pagos adicionales no pueden igualar o superar el total.");
        return;
      }
      payments = [
        {
          method,
          amount: computedPrimaryAmount.toFixed(2),
          recipientAccount:
            method === "transfer" ? recipientAccount || undefined : undefined,
        },
        ...additionalPayments,
      ];
    } else {
      payments = [
        {
          method,
          amount: totalAmount.trim(),
          recipientAccount:
            method === "transfer" ? recipientAccount || undefined : undefined,
        },
      ];
    }

    // Validate transfer has recipient
    for (const part of payments) {
      if (part.method === "transfer" && !part.recipientAccount) {
        setError(
          "Las transferencias requieren seleccionar una cuenta de destino.",
        );
        return;
      }
    }

    try {
      setSaving(true);
      setError(null);
      setSuccess(null);
      await recordSale(
        {
          totalAmount: totalAmount.trim(),
          category,
          detail: detail.trim() || undefined,
          payments,
        },
        generateIdempotencyKey(),
      );
      setSuccess("Venta registrada");
      resetForm();
      onRecorded();
    } catch (err: unknown) {
      const message = typeof err === "string" ? err : String(err);
      setError(`No se pudo registrar la venta. ${message}`);
    } finally {
      setSaving(false);
    }
  };

  return (
    <form onSubmit={handleSubmit} className="space-y-5">
      <h3 className="text-lg font-semibold">Registrar venta</h3>

      {/* Total amount */}
      <div>
        <label className="text-sm font-semibold" htmlFor="sale-total">
          Monto total
        </label>
        <div className="relative mt-2">
          <span className="pointer-events-none absolute inset-y-0 left-3 flex items-center font-mono text-foreground-tertiary">
            $
          </span>
          <input
            className={`${inputClassName} pl-8`}
            disabled={saving}
            id="sale-total"
            inputMode="decimal"
            onChange={(event) => setTotalAmount(event.target.value)}
            placeholder="0.00"
            type="text"
            value={totalAmount}
          />
        </div>
      </div>

      {/* Category */}
      <div>
        <p className="text-sm font-semibold">Rubro</p>
        <div className="mt-2 flex gap-2">
          {CATEGORIES.map((cat) => (
            <Button
              key={cat.value}
              disabled={saving}
              onClick={() => setCategory(cat.value)}
              size="sm"
              type="button"
              variant={category === cat.value ? "default" : "outline"}
            >
              {cat.label}
            </Button>
          ))}
        </div>
      </div>

      {/* Payment method — always visible, acts as primary */}
      <div>
        <p className="text-sm font-semibold">Medio de pago</p>
        <div className="mt-2 flex flex-wrap gap-2">
          {PAYMENT_METHODS.map((pm) => (
            <Button
              key={pm.value}
              disabled={saving}
              onClick={() => setMethod(pm.value)}
              size="sm"
              type="button"
              variant={method === pm.value ? "default" : "outline"}
            >
              {pm.label}
            </Button>
          ))}
        </div>

        {/* Recipient account for transfer */}
        {method === "transfer" && (
          <div className="mt-3">
            <p className="text-sm font-semibold">Cuenta destino</p>
            <div className="mt-2 flex gap-2">
              {RECIPIENT_ACCOUNTS.map((acc) => (
                <Button
                  key={acc}
                  disabled={saving}
                  onClick={() => setRecipientAccount(acc)}
                  size="sm"
                  type="button"
                  variant={recipientAccount === acc ? "default" : "outline"}
                >
                  {acc}
                </Button>
              ))}
            </div>
          </div>
        )}
      </div>

      {/* Additional payments section */}
      {(showAddForm || additionalPayments.length > 0) && (
        <div className="space-y-3">
          <p className="text-sm font-semibold">Pagos adicionales</p>

          {/* List of additional payment parts */}
          {additionalPayments.length > 0 && (
            <div className="space-y-2">
              {additionalPayments.map((part, index) => (
                <div
                  key={index}
                  className="flex items-center justify-between rounded-sm border border-border p-3"
                >
                  <span className="text-sm">
                    {
                      PAYMENT_METHODS.find((m) => m.value === part.method)
                        ?.label
                    }{" "}
                    ${part.amount}
                    {part.recipientAccount && ` → ${part.recipientAccount}`}
                  </span>
                  <Button
                    disabled={saving}
                    onClick={() => removeAdditionalPayment(index)}
                    size="icon-xs"
                    type="button"
                    variant="ghost"
                  >
                    ✕
                  </Button>
                </div>
              ))}
            </div>
          )}

          {/* Add additional payment form */}
          {showAddForm && (
            <div className="flex flex-wrap items-end gap-2">
              <div>
                <label className="text-xs font-semibold" htmlFor="add-method">
                  Método
                </label>
                <select
                  className="mt-1 h-9 rounded-sm border border-input bg-muted px-2 text-sm"
                  id="add-method"
                  onChange={(event) =>
                    setAddMethod(event.target.value as PaymentMethod)
                  }
                  value={addMethod}
                >
                  <option value="">Seleccionar…</option>
                  {PAYMENT_METHODS.map((pm) => (
                    <option key={pm.value} value={pm.value}>
                      {pm.label}
                    </option>
                  ))}
                </select>
              </div>
              <div>
                <label className="text-xs font-semibold" htmlFor="add-amount">
                  Importe
                </label>
                <input
                  className="mt-1 h-9 w-28 rounded-sm border border-input bg-muted px-2 font-mono text-sm"
                  id="add-amount"
                  inputMode="decimal"
                  onChange={(event) => setAddAmount(event.target.value)}
                  placeholder="0.00"
                  type="text"
                  value={addAmount}
                />
              </div>
              {addMethod === "transfer" && (
                <div>
                  <label
                    className="text-xs font-semibold"
                    htmlFor="add-recipient"
                  >
                    Destino
                  </label>
                  <select
                    className="mt-1 h-9 rounded-sm border border-input bg-muted px-2 text-sm"
                    id="add-recipient"
                    onChange={(event) => setAddRecipient(event.target.value)}
                    value={addRecipient}
                  >
                    <option value="">Seleccionar…</option>
                    {RECIPIENT_ACCOUNTS.map((acc) => (
                      <option key={acc} value={acc}>
                        {acc}
                      </option>
                    ))}
                  </select>
                </div>
              )}
              <Button
                disabled={
                  !addMethod ||
                  !addAmount.trim() ||
                  !Number.isFinite(parsedAddAmount) ||
                  parsedAddAmount <= 0 ||
                  (addMethod === "transfer" && !addRecipient) ||
                  displayedPrimaryAmount <= 0
                }
                onClick={addAdditionalPayment}
                size="sm"
                type="button"
                variant="secondary"
              >
                Agregar
              </Button>
            </div>
          )}
        </div>
      )}

      {/* Split payment allocation summary */}
      {isSplitMode && method && (
        <section
          aria-labelledby="split-payment-summary-title"
          aria-live="polite"
          className="space-y-3 rounded-sm border border-border bg-muted/30 p-3"
        >
          <div className="flex items-baseline justify-between gap-4">
            <h4
              className="text-sm font-semibold"
              id="split-payment-summary-title"
            >
              Resumen del pago dividido
            </h4>
            <span className="font-mono text-base font-semibold tabular-nums">
              ${expectedTotal.toFixed(2)}
            </span>
          </div>
          <p className="text-xs text-foreground-secondary">Total de la venta</p>

          <div className="space-y-2 border-t border-border-subtle pt-3">
            <div className="flex items-start justify-between gap-4">
              <div>
                <p className="text-sm font-semibold">Pago principal</p>
                <p className="text-sm">
                  {paymentMethodLabel(method)}
                  {method === "transfer" &&
                    recipientAccount &&
                    ` → ${recipientAccount}`}
                </p>
                <p className="text-xs text-foreground-secondary">
                  Importe restante (calculado automáticamente)
                </p>
              </div>
              <span className="font-mono text-sm font-semibold tabular-nums">
                ${displayedPrimaryAmount.toFixed(2)}
              </span>
            </div>

            {additionalPayments.map((part, index) => (
              <div
                className="flex items-start justify-between gap-4"
                key={`${part.method}-${index}`}
              >
                <div>
                  <p className="text-sm">{paymentMethodLabel(part.method)}</p>
                  {part.recipientAccount && (
                    <p className="text-xs text-foreground-secondary">
                      Destino: {part.recipientAccount}
                    </p>
                  )}
                </div>
                <span className="font-mono text-sm tabular-nums">
                  ${parseFloat(part.amount).toFixed(2)}
                </span>
              </div>
            ))}

            {addMethod && pendingAdditionalAmount > 0 && (
              <div className="flex items-start justify-between gap-4">
                <div>
                  <p className="text-sm">{paymentMethodLabel(addMethod)}</p>
                  <p className="text-xs text-foreground-secondary">
                    Importe adicional
                    {addMethod === "transfer" &&
                      addRecipient &&
                      ` · Destino: ${addRecipient}`}
                  </p>
                </div>
                <span className="font-mono text-sm tabular-nums">
                  ${pendingAdditionalAmount.toFixed(2)}
                </span>
              </div>
            )}
          </div>

          <div className="space-y-1 border-t border-border-subtle pt-3">
            <div className="flex items-baseline justify-between gap-4 text-sm font-semibold">
              <span>Total asignado</span>
              <span className="font-mono tabular-nums">
                ${assignedTotal.toFixed(2)} de ${expectedTotal.toFixed(2)}
              </span>
            </div>
            <p
              className={`text-xs ${splitPaymentValid ? "text-foreground-secondary" : "text-destructive"}`}
            >
              {splitPaymentValid
                ? "Válido: los importes coinciden con el total de la venta."
                : pendingAdditionalAmountInvalid
                  ? "No válido: el importe adicional debe ser mayor que cero."
                  : "No válido: los pagos adicionales deben dejar un saldo positivo."}
            </p>
          </div>
        </section>
      )}

      {/* Add another method / Collapse */}
      <div className="flex gap-2">
        <Button
          disabled={saving || !method}
          onClick={() => {
            if (isSplitMode) {
              setAdditionalPayments([]);
              setShowAddForm(false);
              setAddMethod("");
              setAddAmount("");
              setAddRecipient("");
            } else {
              setShowAddForm(true);
            }
          }}
          size="sm"
          type="button"
          variant="ghost"
        >
          {isSplitMode ? "Un medio de pago" : "Dividir pago"}
        </Button>
      </div>

      {/* Detail */}
      <div>
        <label className="text-sm font-semibold" htmlFor="sale-detail">
          Detalle{" "}
          <span className="font-normal text-foreground-tertiary">
            (opcional)
          </span>
        </label>
        <input
          className="mt-2 h-10 w-full rounded-sm border border-input bg-muted px-3 text-sm outline-none transition-colors duration-150 placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/35"
          disabled={saving}
          id="sale-detail"
          onChange={(event) => setDetail(event.target.value)}
          placeholder="Descripción libre…"
          type="text"
          value={detail}
        />
      </div>

      {/* Error and success messages */}
      {error && (
        <p className="text-sm text-destructive" role="alert">
          {error}
        </p>
      )}
      {success && (
        <p className="text-sm text-primary" role="status">
          {success}
        </p>
      )}

      {/* Submit */}
      <Button
        className="w-full"
        disabled={
          saving ||
          !totalAmount.trim() ||
          !category ||
          !method ||
          !primaryAmountValid
        }
        size="lg"
        type="submit"
      >
        {saving ? "Registrando…" : "Registrar venta"}
      </Button>
    </form>
  );
}
