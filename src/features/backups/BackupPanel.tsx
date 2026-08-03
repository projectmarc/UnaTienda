import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";
import { Button } from "@/components/ui/button";
import {
  exportBackup,
  getBackupPanel,
  restoreBackup,
  selectBackup,
  type BackupPanelResponse,
  type BackupStatus,
  type BackupSummary,
} from "../../shared/tauri/commands";
import { formatDateTime } from "../../shared/dateTime";

function formatBackupDate(value: string): string {
  if (value === "desconocido") return value;
  return formatDateTime(value);
}

function formatBytes(value?: number): string {
  if (value === undefined) return "";
  if (value < 1024) return `${value} B`;
  return `${(value / 1024 / 1024).toFixed(1)} MB`;
}

function backupKindLabel(kind: BackupStatus["kind"]): string {
  switch (kind) {
    case "automatic":
      return "Automática (legado)";
    case "daily":
      return "Diaria";
    case "session_close":
      return "Cierre de caja";
    case "pre_restore":
      return "Previo a restauración";
    case "manual_export":
      return "Exportación manual";
  }
}

function backupFailureMessage(error: unknown): string {
  const message = String(error);
  if (message.includes("RESTORE_BLOCKED_OPEN_SESSION")) {
    return "Cierra la sesión de caja abierta antes de restaurar una copia.";
  }
  if (message.includes("INCOMPATIBLE_BACKUP")) {
    return "La copia no supera la validación de integridad o no pertenece a UnaTienda.";
  }
  if (message.includes("DATABASE_UNAVAILABLE")) {
    return "La base de datos quedó protegida durante una recuperación. Reinicia la aplicación para continuar.";
  }
  return "No se pudo completar la operación. Puedes reintentarlo sin perder la información actual.";
}

function RestoreSummary({ summary }: { summary: BackupSummary }) {
  return (
    <dl className="mt-4 grid gap-3 border-y border-border-subtle py-4 text-sm sm:grid-cols-2">
      <div>
        <dt className="text-foreground-tertiary">Archivo</dt>
        <dd className="mt-1 break-all font-mono text-xs">{summary.fileName}</dd>
      </div>
      <div>
        <dt className="text-foreground-tertiary">Fecha de la copia</dt>
        <dd className="mt-1">{formatBackupDate(summary.backupTimestamp)}</dd>
      </div>
      <div>
        <dt className="text-foreground-tertiary">Versión del esquema</dt>
        <dd className="mt-1 font-mono">{summary.schemaVersion}</dd>
      </div>
      <div>
        <dt className="text-foreground-tertiary">
          Sesiones cerradas o abiertas
        </dt>
        <dd className="mt-1 font-mono">{summary.sessionCount}</dd>
      </div>
      <div>
        <dt className="text-foreground-tertiary">Ventas</dt>
        <dd className="mt-1 font-mono">{summary.saleCount}</dd>
      </div>
      <div>
        <dt className="text-foreground-tertiary">Movimientos y devoluciones</dt>
        <dd className="mt-1 font-mono">
          {summary.movementCount} · {summary.returnCount}
        </dd>
      </div>
    </dl>
  );
}

function BackupRow({ backup }: { backup: BackupStatus }) {
  return (
    <li className="border-b border-border-subtle py-3 last:border-b-0">
      <div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1">
        <span className="font-medium">{backupKindLabel(backup.kind)}</span>
        <time
          className="font-mono text-xs tabular-nums text-foreground-tertiary"
          dateTime={backup.runAt}
        >
          {formatDateTime(backup.runAt)}
        </time>
      </div>
      <div className="mt-1 flex flex-wrap items-center justify-between gap-2 text-xs text-foreground-secondary">
        <span className="break-all font-mono">{backup.fileName}</span>
        <span
          className={
            backup.success && backup.available
              ? "text-success"
              : "text-destructive"
          }
        >
          {backup.success && backup.available
            ? `Disponible${backup.sizeBytes ? ` · ${formatBytes(backup.sizeBytes)}` : ""}`
            : backup.success
              ? "Archivo no disponible"
              : "Falló la creación"}
        </span>
      </div>
    </li>
  );
}

export default function BackupPanel() {
  const [panel, setPanel] = useState<BackupPanelResponse | null>(null);
  const [selected, setSelected] = useState<BackupSummary | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const [loading, setLoading] = useState(true);
  const [working, setWorking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const restoreTriggerRef = useRef<HTMLButtonElement>(null);
  const restoreDialogRef = useRef<HTMLDivElement>(null);
  const returnFocusRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (!selected) {
      if (!working) {
        returnFocusRef.current?.focus();
        returnFocusRef.current = null;
      }
      return;
    }

    if (!returnFocusRef.current) {
      returnFocusRef.current =
        document.activeElement instanceof HTMLElement
          ? document.activeElement
          : restoreTriggerRef.current;
    }
    const firstFocusable = restoreDialogRef.current?.querySelector<HTMLElement>(
      'input, button, [tabindex]:not([tabindex="-1"])',
    );
    firstFocusable?.focus();
  }, [selected, working]);

  const handleRestoreDialogKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape" && !working) {
      setSelected(null);
      setConfirmed(false);
      return;
    }
    if (event.key !== "Tab") return;

    const focusable = Array.from(
      restoreDialogRef.current?.querySelectorAll<HTMLElement>(
        'input, button, [tabindex]:not([tabindex="-1"])',
      ) ?? [],
    );
    if (focusable.length === 0) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  };

  const loadPanel = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      setPanel(await getBackupPanel());
    } catch (loadError) {
      setError(backupFailureMessage(loadError));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadPanel();
  }, [loadPanel]);

  const handleExport = async () => {
    try {
      setWorking(true);
      setError(null);
      setNotice(null);
      const result = await exportBackup();
      if (result) setNotice(`Copia exportada como ${result.fileName}.`);
    } catch (exportError) {
      setError(backupFailureMessage(exportError));
    } finally {
      setWorking(false);
    }
  };

  const handleSelect = async () => {
    try {
      setWorking(true);
      setError(null);
      setNotice(null);
      const result = await selectBackup();
      if (result) {
        setSelected(result.summary);
        setConfirmed(false);
      }
    } catch (selectError) {
      setError(backupFailureMessage(selectError));
    } finally {
      setWorking(false);
    }
  };

  const handleRestore = async () => {
    if (!selected || !confirmed) return;
    try {
      setWorking(true);
      setError(null);
      setNotice(null);
      const result = await restoreBackup();
      setSelected(null);
      setConfirmed(false);
      setNotice(
        result.restartRequired
          ? "La restauración terminó. Reinicia la aplicación antes de continuar."
          : "La restauración terminó y la información restaurada ya está activa.",
      );
      await loadPanel();
    } catch (restoreError) {
      setError(backupFailureMessage(restoreError));
    } finally {
      setWorking(false);
    }
  };

  return (
    <section aria-labelledby="backup-title" className="py-8">
      <header className="border-b border-border pb-5">
        <p className="text-xs font-semibold tracking-[0.16em] text-primary uppercase">
          Resguardo local
        </p>
        <h2
          id="backup-title"
          className="mt-2 text-3xl font-semibold tracking-tight"
        >
          Copias de seguridad
        </h2>
        <p className="mt-2 max-w-2xl leading-7 text-foreground-secondary">
          UnaTienda crea una copia diaria al iniciar y otra después de cada
          cierre de caja. Las copias internas se conservan durante 30 días; las
          exportadas quedan bajo tu control.
        </p>
      </header>

      <div className="mt-8 grid gap-6 lg:grid-cols-[minmax(0,1.05fr)_minmax(18rem,0.95fr)]">
        <div className="border border-border bg-card p-5">
          <div className="flex flex-wrap items-start justify-between gap-4">
            <div>
              <p className="text-xs font-semibold tracking-[0.14em] text-foreground-tertiary uppercase">
                Estado
              </p>
              <h3 className="mt-1 text-xl font-semibold">
                Protección automática
              </h3>
            </div>
            <span className="rounded-sm bg-accent px-2 py-1 text-xs font-semibold text-accent-foreground">
              Interna · 30 días
            </span>
          </div>

          {loading && (
            <p className="mt-8 text-sm text-foreground-secondary" role="status">
              Consultando el estado de las copias…
            </p>
          )}
          {!loading && panel?.lastSuccessful && (
            <div className="mt-6 border-l-2 border-success pl-4">
              <p className="text-sm text-foreground-tertiary">
                Última copia exitosa
              </p>
              <p className="mt-1 font-medium">
                {formatDateTime(panel.lastSuccessful.runAt)} ·{" "}
                {backupKindLabel(panel.lastSuccessful.kind)}
              </p>
              <p className="mt-1 break-all font-mono text-xs text-foreground-secondary">
                {panel.lastSuccessful.fileName}
              </p>
            </div>
          )}
          {!loading && !panel?.lastSuccessful && (
            <p className="mt-8 border border-border-subtle bg-secondary p-4 text-sm text-foreground-secondary">
              Todavía no hay una copia exitosa registrada. Puedes exportar una
              ahora.
            </p>
          )}

          {error && (
            <p className="mt-5 text-sm text-destructive" role="alert">
              {error}
            </p>
          )}
          {notice && (
            <p className="mt-5 text-sm text-success" role="status">
              {notice}
            </p>
          )}

          <div className="mt-6 flex flex-wrap gap-3">
            <Button disabled={working} onClick={handleExport} type="button">
              {working ? "Procesando…" : "Exportar una copia"}
            </Button>
            <Button
              disabled={working}
              onClick={handleSelect}
              ref={restoreTriggerRef}
              type="button"
              variant="outline"
            >
              Elegir copia para restaurar
            </Button>
            <Button
              disabled={working}
              onClick={loadPanel}
              type="button"
              variant="ghost"
            >
              Actualizar estado
            </Button>
          </div>
        </div>

        <div className="border border-border bg-secondary p-5">
          <p className="text-xs font-semibold tracking-[0.14em] text-foreground-tertiary uppercase">
            Registro interno
          </p>
          <h3 className="mt-1 text-xl font-semibold">Copias administradas</h3>
          <p className="mt-2 text-sm leading-6 text-foreground-secondary">
            Las copias fallidas quedan visibles para que puedas reintentar. Los
            archivos exportados no aparecen aquí ni se eliminan automáticamente.
          </p>
          {!loading && panel && panel.backups.length === 0 && (
            <p className="mt-6 text-sm text-foreground-tertiary">
              Registro vacío.
            </p>
          )}
          {panel && panel.backups.length > 0 && (
            <ol aria-label="Copias administradas" className="mt-5">
              {panel.backups.map((backup) => (
                <BackupRow backup={backup} key={backup.id} />
              ))}
            </ol>
          )}
        </div>
      </div>

      {selected && (
        <div
          aria-labelledby="restore-confirm-title"
          aria-describedby="restore-confirm-description"
          aria-modal="true"
          className="mt-6 border border-border-strong bg-card p-5"
          onKeyDown={handleRestoreDialogKeyDown}
          ref={restoreDialogRef}
          role="alertdialog"
        >
          <p className="text-xs font-semibold tracking-[0.14em] text-destructive uppercase">
            Confirmación requerida
          </p>
          <h3 id="restore-confirm-title" className="mt-1 text-xl font-semibold">
            Restaurar esta copia
          </h3>
          <p
            className="mt-2 max-w-2xl text-sm leading-6 text-foreground-secondary"
            id="restore-confirm-description"
          >
            Antes de reemplazar la información actual se guardará y validará una
            copia preventiva. La restauración no se permite con una caja
            abierta.
          </p>
          <RestoreSummary summary={selected} />
          <label className="mt-4 flex items-start gap-3 text-sm">
            <input
              checked={confirmed}
              className="mt-0.5 size-4 accent-primary"
              onChange={(event) => setConfirmed(event.target.checked)}
              type="checkbox"
            />
            <span>
              Entiendo que la información actual será reemplazada por esta
              copia.
            </span>
          </label>
          <div className="mt-5 flex flex-wrap gap-3">
            <Button
              disabled={!confirmed || working}
              onClick={handleRestore}
              type="button"
              variant="destructive"
            >
              {working ? "Restaurando…" : "Confirmar restauración"}
            </Button>
            <Button
              disabled={working}
              onClick={() => {
                setSelected(null);
                setConfirmed(false);
              }}
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
