import { useState } from "react";
import { Button } from "@/components/ui/button";
import { recordCashOut } from "../../shared/tauri/commands";

function generateIdempotencyKey(): string {
  return crypto.randomUUID();
}

const inputClassName =
  "h-12 w-full rounded-sm border border-input bg-muted px-3 font-mono text-lg tabular-nums outline-none transition-colors duration-150 placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/35 disabled:cursor-not-allowed disabled:opacity-50";

interface CashOutFormProps {
  onRecorded: () => void;
}

export default function CashOutForm({ onRecorded }: CashOutFormProps) {
  const [amount, setAmount] = useState("");
  const [detail, setDetail] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);

  const resetForm = () => {
    setAmount("");
    setDetail("");
    setError(null);
  };

  const handleSubmit = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!amount.trim()) return;

    try {
      setSaving(true);
      setError(null);
      setSuccess(null);
      await recordCashOut(
        amount.trim(),
        detail.trim() || undefined,
        generateIdempotencyKey(),
      );
      setSuccess("Salida de caja registrada");
      resetForm();
      onRecorded();
    } catch (err: unknown) {
      const message = typeof err === "string" ? err : String(err);
      setError(`No se pudo registrar la salida. ${message}`);
    } finally {
      setSaving(false);
    }
  };

  return (
    <form onSubmit={handleSubmit} className="space-y-5">
      <h3 className="text-lg font-semibold">Salida de caja</h3>

      {/* Amount */}
      <div>
        <label className="text-sm font-semibold" htmlFor="cashout-amount">
          Importe
        </label>
        <div className="relative mt-2">
          <span className="pointer-events-none absolute inset-y-0 left-3 flex items-center font-mono text-foreground-tertiary">
            $
          </span>
          <input
            className={`${inputClassName} pl-8`}
            disabled={saving}
            id="cashout-amount"
            inputMode="decimal"
            onChange={(event) => setAmount(event.target.value)}
            placeholder="0.00"
            type="text"
            value={amount}
          />
        </div>
      </div>

      {/* Detail */}
      <div>
        <label className="text-sm font-semibold" htmlFor="cashout-detail">
          Motivo{" "}
          <span className="font-normal text-foreground-tertiary">(opcional)</span>
        </label>
        <input
          className="mt-2 h-10 w-full rounded-sm border border-input bg-muted px-3 text-sm outline-none transition-colors duration-150 placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/35"
          disabled={saving}
          id="cashout-detail"
          onChange={(event) => setDetail(event.target.value)}
          placeholder="Ej: compra de insumos, cambio…"
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
        disabled={saving || !amount.trim()}
        size="lg"
        type="submit"
        variant="secondary"
      >
        {saving ? "Registrando…" : "Registrar salida"}
      </Button>
    </form>
  );
}
