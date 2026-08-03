import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import BackupPanel from "./BackupPanel";

vi.mock("../../shared/tauri/commands", () => ({
  exportBackup: vi.fn(),
  getBackupPanel: vi.fn(),
  restoreBackup: vi.fn(),
  selectBackup: vi.fn(),
}));

import {
  exportBackup,
  getBackupPanel,
  restoreBackup,
  selectBackup,
} from "../../shared/tauri/commands";

const mockGetBackupPanel = vi.mocked(getBackupPanel);
const mockExportBackup = vi.mocked(exportBackup);
const mockRestoreBackup = vi.mocked(restoreBackup);
const mockSelectBackup = vi.mocked(selectBackup);

const summary = {
  backupTimestamp: "2026-08-03T18:00:00Z",
  fileName: "unatienda-daily-20260803.db",
  schemaVersion: 6,
  sessionCount: 4,
  saleCount: 12,
  movementCount: 3,
  returnCount: 1,
};

describe("BackupPanel", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mockGetBackupPanel.mockResolvedValue({
      lastSuccessful: {
        id: "backup-1",
        runAt: "2026-08-03T18:00:00Z",
        kind: "daily",
        success: true,
        available: true,
        fileName: summary.fileName,
        sizeBytes: 4096,
      },
      backups: [
        {
          id: "backup-1",
          runAt: "2026-08-03T18:00:00Z",
          kind: "daily",
          success: true,
          available: true,
          fileName: summary.fileName,
          sizeBytes: 4096,
        },
        {
          id: "backup-2",
          runAt: "2026-08-02T18:00:00Z",
          kind: "session_close",
          success: false,
          available: false,
          fileName: "unatienda-session-close-failed.db",
          error: "disk full",
        },
      ],
    });
  });

  it("shows the latest backup and failed managed runs", async () => {
    render(<BackupPanel />);

    expect(await screen.findByText("Última copia exitosa")).toBeInTheDocument();
    expect(screen.getByText("Cierre de caja")).toBeInTheDocument();
    expect(screen.getByText("Falló la creación")).toBeInTheDocument();
    expect(screen.getAllByText("unatienda-daily-20260803.db")).toHaveLength(2);
  });

  it("requires explicit confirmation before restoring a validated backup", async () => {
    const user = userEvent.setup();
    mockSelectBackup.mockResolvedValue({ summary });
    mockRestoreBackup.mockResolvedValue({
      summary,
      restartRequired: false,
    });

    render(<BackupPanel />);
    await user.click(
      screen.getByRole("button", { name: "Elegir copia para restaurar" }),
    );

    expect(
      await screen.findByRole("heading", { name: "Restaurar esta copia" }),
    ).toBeInTheDocument();
    const restoreDialog = screen.getByRole("alertdialog");
    expect(
      within(restoreDialog).getByText(summary.fileName),
    ).toBeInTheDocument();
    const restoreTrigger = screen.getByRole("button", {
      name: "Elegir copia para restaurar",
    });
    expect(
      screen.getByRole("checkbox", {
        name: "Entiendo que la información actual será reemplazada por esta copia.",
      }),
    ).toHaveFocus();
    const restoreButton = screen.getByRole("button", {
      name: "Confirmar restauración",
    });
    expect(restoreButton).toBeDisabled();

    await user.click(
      screen.getByRole("checkbox", {
        name: "Entiendo que la información actual será reemplazada por esta copia.",
      }),
    );
    expect(restoreButton).toBeEnabled();
    await user.click(restoreButton);

    expect(mockRestoreBackup).toHaveBeenCalledOnce();
    expect(
      await screen.findByText(
        "La restauración terminó y la información restaurada ya está activa.",
      ),
    ).toBeInTheDocument();
    await waitFor(() => expect(restoreTrigger).toHaveFocus());
  });

  it("exports a fresh validated snapshot", async () => {
    const user = userEvent.setup();
    mockExportBackup.mockResolvedValue(summary);

    render(<BackupPanel />);
    await user.click(
      screen.getByRole("button", { name: "Exportar una copia" }),
    );

    expect(mockExportBackup).toHaveBeenCalledOnce();
    expect(
      await screen.findByText(
        "Copia exportada como unatienda-daily-20260803.db.",
      ),
    ).toBeInTheDocument();
  });
});
