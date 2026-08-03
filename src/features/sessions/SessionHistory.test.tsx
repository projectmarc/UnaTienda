import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import SessionHistory from "./SessionHistory";

vi.mock("../../shared/tauri/commands", () => ({
  annulSale: vi.fn(),
}));

import { annulSale } from "../../shared/tauri/commands";

const mockAnnulSale = vi.mocked(annulSale);

const history = {
  operations: [
    {
      id: "sale-1",
      occurredAt: "2026-08-03T10:15:00Z",
      operationType: "sale" as const,
      amount: "120.00",
      detail: "Campera y cuaderno",
      category: "mixto" as const,
      payments: [
        { method: "cash" as const, amount: "70.00" },
        {
          method: "transfer" as const,
          amount: "30.00",
          recipientAccount: "Juan",
        },
        {
          method: "transfer" as const,
          amount: "20.00",
          recipientAccount: "Carlos",
        },
      ],
      status: "confirmed" as const,
    },
    {
      id: "movement-1",
      occurredAt: "2026-08-03T10:30:00Z",
      operationType: "cash_out" as const,
      amount: "20.00",
      detail: "Compra de insumos",
      payments: [],
    },
  ],
  paymentTotals: {
    cash: "70.00",
    transfer: "50.00",
    debit: "0.00",
    credit: "0.00",
    qr: "0.00",
  },
  transferTotals: [
    { recipientAccount: "Juan", amount: "30.00" },
    { recipientAccount: "Carlos", amount: "20.00" },
  ],
  expectedCash: "150.00",
};

describe("SessionHistory", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("shows operation details, payment breakdown, and net totals", () => {
    render(<SessionHistory history={history} onAnnulled={vi.fn()} />);

    const operations = screen.getByRole("list", {
      name: "Operaciones de la sesión",
    });
    expect(operations.children).toHaveLength(2);
    expect(screen.getByText("Campera y cuaderno")).toBeInTheDocument();
    expect(screen.getByText("Rubro: Mixto")).toBeInTheDocument();
    const paymentBreakdown = screen.getByRole("list", {
      name: "Desglose del pago",
    });
    const paymentRows = within(paymentBreakdown).getAllByRole("listitem");
    expect(paymentRows).toHaveLength(3);
    expect(paymentRows[0].children).toHaveLength(2);
    expect(paymentRows[0].children[0]).toHaveTextContent("Efectivo");
    expect(paymentRows[0].children[1]).toHaveTextContent("$ 70,00");
    expect(paymentRows[1].children).toHaveLength(3);
    expect(paymentRows[1].children[0]).toHaveTextContent("Transferencia");
    expect(paymentRows[1].children[1]).toHaveTextContent("Destino: Juan");
    expect(paymentRows[1].children[2]).toHaveTextContent("$ 30,00");
    expect(paymentRows[2].children).toHaveLength(3);
    expect(paymentRows[2].children[0]).toHaveTextContent("Transferencia");
    expect(paymentRows[2].children[1]).toHaveTextContent("Destino: Carlos");
    expect(paymentRows[2].children[2]).toHaveTextContent("$ 20,00");
    expect(screen.getByText("Compra de insumos")).toBeInTheDocument();
    expect(screen.getByText("Efectivo esperado")).toBeInTheDocument();
    expect(screen.getByText("$ 150,00")).toBeInTheDocument();
    const transferBreakdown = screen.getByRole("group", {
      name: "Transferencias por cuenta",
    });
    expect(within(transferBreakdown).getByText("Juan")).toBeInTheDocument();
    expect(within(transferBreakdown).getByText("Carlos")).toBeInTheDocument();
    expect(within(transferBreakdown).getByText("$ 30,00")).toBeInTheDocument();
    expect(within(transferBreakdown).getByText("$ 20,00")).toBeInTheDocument();
    expect(screen.getAllByText("$ 70,00")).not.toHaveLength(0);
    expect(screen.getAllByText("$ 50,00")).not.toHaveLength(0);
  });

  it("requires confirmation before annulling and sends the optional reason", async () => {
    const user = userEvent.setup();
    const onAnnulled = vi.fn();
    mockAnnulSale.mockResolvedValue({ saleId: "sale-1", status: "annulled" });

    render(<SessionHistory history={history} onAnnulled={onAnnulled} />);

    await user.click(screen.getByRole("button", { name: "Anular venta" }));
    expect(mockAnnulSale).not.toHaveBeenCalled();
    expect(screen.getByRole("alertdialog")).toBeInTheDocument();

    const dialog = screen.getByRole("alertdialog");
    await user.type(
      within(dialog).getByLabelText(/Motivo/),
      "pedido del cliente",
    );
    await user.click(
      within(dialog).getByRole("button", { name: "Confirmar anulación" }),
    );

    expect(mockAnnulSale).toHaveBeenCalledWith(
      { saleId: "sale-1", reason: "pedido del cliente" },
      expect.any(String),
    );
    expect(onAnnulled).toHaveBeenCalledTimes(1);
  });

  it("can cancel the confirmation without mutating the sale", async () => {
    const user = userEvent.setup();

    render(<SessionHistory history={history} onAnnulled={vi.fn()} />);

    await user.click(screen.getByRole("button", { name: "Anular venta" }));
    await user.click(screen.getByRole("button", { name: "Cancelar" }));

    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(mockAnnulSale).not.toHaveBeenCalled();
  });
});
