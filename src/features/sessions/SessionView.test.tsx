import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import SessionView from "./SessionView";

vi.mock("../../shared/tauri/commands", () => ({
  openSession: vi.fn(),
  getReconciliation: vi.fn(),
  getSessionHistory: vi.fn(),
  closeSession: vi.fn(),
  recordSale: vi.fn(),
  recordCashOut: vi.fn(),
  annulSale: vi.fn(),
}));

import {
  closeSession,
  getReconciliation,
  getSessionHistory,
  openSession,
  annulSale,
  recordSale,
} from "../../shared/tauri/commands";

const mockOpenSession = vi.mocked(openSession);
const mockGetReconciliation = vi.mocked(getReconciliation);
const mockCloseSession = vi.mocked(closeSession);
const mockGetSessionHistory = vi.mocked(getSessionHistory);
const mockAnnulSale = vi.mocked(annulSale);
const mockRecordSale = vi.mocked(recordSale);

const openReconciliation = {
  sessionId: "test-session",
  openingAmount: "100.00",
  expectedCash: "100.00",
};

describe("SessionView", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockGetSessionHistory.mockResolvedValue({
      operations: [],
      paymentTotals: {
        cash: "0.00",
        transfer: "0.00",
        debit: "0.00",
        credit: "0.00",
        qr: "0.00",
      },
      transferTotals: [],
      expectedCash: "100.00",
    });
  });

  it("muestra el estado de carga inicial", () => {
    mockGetReconciliation.mockReturnValue(new Promise(() => {}));

    render(<SessionView />);

    expect(
      screen.getByText("Consultando la sesión de caja…"),
    ).toBeInTheDocument();
  });

  it("abre una sesión y muestra la fase de operación", async () => {
    const user = userEvent.setup();
    mockGetReconciliation
      .mockRejectedValueOnce("NO_OPEN_SESSION")
      .mockResolvedValueOnce({
        ...openReconciliation,
        openingAmount: "50.00",
        expectedCash: "50.00",
      });
    mockOpenSession.mockResolvedValue({
      sessionId: "test-session",
      openingAmount: "50.00",
    });

    render(<SessionView />);

    await user.type(await screen.findByLabelText("Efectivo inicial"), "50.00");
    await user.click(screen.getByRole("button", { name: "Abrir caja" }));

    await waitFor(() => {
      expect(mockOpenSession).toHaveBeenCalledWith("50.00", expect.any(String));
      expect(mockGetReconciliation).toHaveBeenCalledTimes(2);
    });
    expect(
      await screen.findByRole("heading", { name: "Registrar movimientos" }),
    ).toBeInTheDocument();
  });

  it("reutiliza la clave de apertura al reintentar un error recuperable", async () => {
    const user = userEvent.setup();
    mockGetReconciliation.mockRejectedValue("NO_OPEN_SESSION");
    mockOpenSession
      .mockRejectedValueOnce("CONNECTION_LOST")
      .mockRejectedValueOnce("CONNECTION_LOST");

    render(<SessionView />);

    await user.type(await screen.findByLabelText("Efectivo inicial"), "25.00");
    await user.click(screen.getByRole("button", { name: "Abrir caja" }));
    await screen.findByRole("alert");
    await user.click(
      screen.getByRole("button", { name: "Reintentar apertura" }),
    );

    await waitFor(() => expect(mockOpenSession).toHaveBeenCalledTimes(2));
    expect(mockOpenSession.mock.calls[0][1]).toBe(
      mockOpenSession.mock.calls[1][1],
    );
  });

  it("navega a conciliación y solicita la diferencia exacta", async () => {
    const user = userEvent.setup();
    mockGetReconciliation
      .mockResolvedValueOnce(openReconciliation)
      .mockResolvedValueOnce({
        ...openReconciliation,
        countedCash: "90.00",
        difference: "-10.00",
      });

    render(<SessionView />);

    // First, navigate to reconciliation phase
    await user.click(
      await screen.findByRole("button", { name: "Proceder al cierre" }),
    );

    // Now we should be in the reconcile phase
    expect(
      screen.getByRole("heading", { name: "Conciliar caja" }),
    ).toBeInTheDocument();

    await user.type(await screen.findByLabelText("Efectivo contado"), "90.00");

    await waitFor(
      () => expect(mockGetReconciliation).toHaveBeenCalledWith("90.00"),
      { timeout: 1000 },
    );
    expect(await screen.findByText("-$ 10,00")).toBeInTheDocument();
  });

  it("exige revisar la conciliación antes de cerrar", async () => {
    const user = userEvent.setup();
    mockGetReconciliation
      .mockResolvedValueOnce(openReconciliation)
      .mockResolvedValueOnce({
        ...openReconciliation,
        countedCash: "105.00",
        difference: "5.00",
      });
    mockCloseSession.mockResolvedValue({
      ...openReconciliation,
      countedCash: "105.00",
      difference: "5.00",
      backupWarning:
        "La caja se cerró correctamente, pero no se pudo crear la copia de seguridad.",
    });

    render(<SessionView />);

    // Navigate to reconciliation
    await user.click(
      await screen.findByRole("button", { name: "Proceder al cierre" }),
    );

    await user.type(await screen.findByLabelText("Efectivo contado"), "105.00");
    await screen.findByText("+$ 5,00");
    await user.click(screen.getByRole("button", { name: "Revisar cierre" }));

    expect(mockCloseSession).not.toHaveBeenCalled();
    expect(
      screen.getByText("Se registrará un sobrante de $ 5,00."),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Confirmar cierre" }));

    expect(
      await screen.findByRole("heading", { name: "Caja cerrada" }),
    ).toBeInTheDocument();
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "La caja permanece cerrada",
    );
    expect(mockCloseSession).toHaveBeenCalledWith("105.00", expect.any(String));

    await user.click(
      screen.getByRole("button", { name: "Abrir una nueva caja" }),
    );
    expect(
      screen.getByRole("heading", { name: "Abrir sesión de caja" }),
    ).toBeInTheDocument();
  });

  it("conserva los datos y permite reintentar cuando falla el cierre", async () => {
    const user = userEvent.setup();
    mockGetReconciliation
      .mockResolvedValueOnce(openReconciliation)
      .mockResolvedValueOnce({
        ...openReconciliation,
        countedCash: "100.00",
        difference: "0.00",
      });
    mockCloseSession.mockRejectedValue("DRAFTS_BLOCK_CLOSE");

    render(<SessionView />);

    // Navigate to reconciliation
    await user.click(
      await screen.findByRole("button", { name: "Proceder al cierre" }),
    );

    const input = await screen.findByLabelText("Efectivo contado");
    await user.type(input, "100.00");
    await screen.findByText("Sin diferencia");
    await user.click(screen.getByRole("button", { name: "Revisar cierre" }));
    await user.click(screen.getByRole("button", { name: "Confirmar cierre" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "No se pudo cerrar la caja.",
    );
    expect(input).toHaveValue("100.00");
    expect(
      screen.getByRole("button", { name: "Reintentar cierre" }),
    ).toBeInTheDocument();
  });

  it("permite volver a la fase de operación desde conciliación", async () => {
    const user = userEvent.setup();
    mockGetReconciliation.mockResolvedValueOnce(openReconciliation);

    render(<SessionView />);

    // Navigate to reconciliation
    await user.click(
      await screen.findByRole("button", { name: "Proceder al cierre" }),
    );

    expect(
      screen.getByRole("heading", { name: "Conciliar caja" }),
    ).toBeInTheDocument();

    // Go back to operation
    await user.click(
      screen.getByRole("button", { name: "Volver a operación" }),
    );

    expect(
      screen.getByRole("heading", { name: "Registrar movimientos" }),
    ).toBeInTheDocument();
  });

  it("refreshes history and totals after recording a sale", async () => {
    const user = userEvent.setup();
    mockGetReconciliation.mockResolvedValue(openReconciliation);
    mockRecordSale.mockResolvedValue({
      saleId: "sale-1",
      totalAmount: "20.00",
    });

    render(<SessionView />);

    await user.type(await screen.findByLabelText("Monto total"), "20.00");
    await user.click(screen.getByRole("button", { name: "Ropa" }));
    await user.click(screen.getByRole("button", { name: "Efectivo" }));
    await user.click(screen.getByRole("button", { name: "Registrar venta" }));

    await waitFor(() => {
      expect(mockRecordSale).toHaveBeenCalled();
      expect(mockGetSessionHistory).toHaveBeenCalledTimes(2);
    });
  });

  it("requires confirmation and refreshes after annulling a sale", async () => {
    const user = userEvent.setup();
    mockGetReconciliation.mockResolvedValue(openReconciliation);
    mockGetSessionHistory.mockResolvedValue({
      operations: [
        {
          id: "sale-1",
          occurredAt: "2026-08-03T00:00:00Z",
          operationType: "sale",
          amount: "20.00",
          category: "ropa",
          payments: [{ method: "cash", amount: "20.00" }],
          status: "confirmed",
        },
      ],
      paymentTotals: {
        cash: "20.00",
        transfer: "0.00",
        debit: "0.00",
        credit: "0.00",
        qr: "0.00",
      },
      transferTotals: [],
      expectedCash: "120.00",
    });
    mockAnnulSale.mockResolvedValue({ saleId: "sale-1", status: "annulled" });

    render(<SessionView />);

    await user.click(
      await screen.findByRole("button", { name: "Anular venta" }),
    );
    expect(mockAnnulSale).not.toHaveBeenCalled();
    expect(screen.getByRole("alertdialog")).toBeInTheDocument();

    await user.type(
      within(screen.getByRole("alertdialog")).getByLabelText(/Motivo/),
      "error de carga",
    );
    await user.click(
      screen.getByRole("button", { name: "Confirmar anulación" }),
    );

    await waitFor(() => {
      expect(mockAnnulSale).toHaveBeenCalledWith(
        { saleId: "sale-1", reason: "error de carga" },
        expect.any(String),
      );
      expect(mockGetSessionHistory).toHaveBeenCalledTimes(2);
    });
  });
});
