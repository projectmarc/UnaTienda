import { useCallback, useEffect, useRef, useState } from "react";
import {
  closeSession,
  getReconciliation,
  type ClosedSessionResponse,
  type ReconciliationView,
} from "../../shared/tauri/commands";

function generateIdempotencyKey(): string {
  return crypto.randomUUID();
}

export default function SessionView() {
  const [session, setSession] = useState<ReconciliationView | null>(null);
  const [countedCash, setCountedCash] = useState("");
  const [closedResult, setClosedResult] = useState<ClosedSessionResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [closing, setClosing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // JD2-B-001 fix: persist the idempotency key across retries for the same close attempt.
  // A new key is generated only when a fresh close begins (not on retry).
  const closeKeyRef = useRef<string | null>(null);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const loadSession = useCallback(async (counted?: string) => {
    try {
      setLoading(true);
      setError(null);
      const data = await getReconciliation(counted);
      setSession(data);
    } catch (err: unknown) {
      const msg = typeof err === "string" ? err : String(err);
      if (msg.includes("NO_OPEN_SESSION") || msg.includes("no open session")) {
        setSession(null);
      } else {
        setError(msg);
      }
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadSession();
  }, [loadSession]);

  // V2-VERIFY-001: compute exact difference via backend when counted cash changes.
  // Debounce to avoid excessive IPC calls while typing.
  useEffect(() => {
    if (debounceRef.current) {
      clearTimeout(debounceRef.current);
    }

    if (!countedCash.trim()) {
      // Clear counted/difference when input is empty.
      // Do NOT reload here — the initial load effect handles the base state.
      setSession((prev) =>
        prev ? { ...prev, countedCash: undefined, difference: undefined } : prev,
      );
      return;
    }

    debounceRef.current = setTimeout(() => {
      loadSession(countedCash.trim());
    }, 300);

    return () => {
      if (debounceRef.current) {
        clearTimeout(debounceRef.current);
      }
    };
  }, [countedCash, loadSession]);

  const handleClose = async () => {
    if (!session || !countedCash.trim()) return;

    // JD2-B-001 fix: reuse the same idempotency key on retry so the backend
    // replays the committed response if the first request actually succeeded
    // but the response was lost.
    if (!closeKeyRef.current) {
      closeKeyRef.current = generateIdempotencyKey();
    }

    try {
      setClosing(true);
      setError(null);
      const result = await closeSession(countedCash.trim(), closeKeyRef.current);
      setClosedResult(result);
      setSession(null);
      closeKeyRef.current = null;
    } catch (err: unknown) {
      setError(typeof err === "string" ? err : String(err));
    } finally {
      setClosing(false);
    }
  };

  if (loading && !session) {
    return (
      <div className="container">
        <p>Loading session...</p>
      </div>
    );
  }

  if (closedResult) {
    return (
      <div className="container">
        <h2>Session Closed</h2>
        <dl>
          <dt>Opening Amount</dt>
          <dd>{closedResult.openingAmount}</dd>
          <dt>Expected Cash</dt>
          <dd>{closedResult.expectedCash}</dd>
          <dt>Counted Cash</dt>
          <dd>{closedResult.countedCash}</dd>
          <dt>Difference</dt>
          <dd>{closedResult.difference}</dd>
        </dl>
      </div>
    );
  }

  if (!session) {
    return (
      <div className="container">
        <p>No open session.</p>
      </div>
    );
  }

  return (
    <div className="container">
      <h2>Open Cash Session</h2>
      <dl>
        <dt>Opening Amount</dt>
        <dd>{session.openingAmount}</dd>
        <dt>Expected Cash</dt>
        <dd>{session.expectedCash}</dd>
      </dl>

      <label htmlFor="counted-cash">Counted Cash</label>
      <input
        id="counted-cash"
        type="text"
        inputMode="decimal"
        placeholder="0.00"
        value={countedCash}
        onChange={(e) => setCountedCash(e.target.value)}
        disabled={closing}
      />

      {/* V2-VERIFY-001: display exact difference from backend, not parseFloat */}
      {session.difference && (
        <p>Difference: {session.difference}</p>
      )}

      {error && <p role="alert">{error}</p>}

      <button
        onClick={handleClose}
        disabled={closing || !countedCash.trim()}
      >
        {closing ? "Closing..." : "Close Session"}
      </button>
    </div>
  );
}
