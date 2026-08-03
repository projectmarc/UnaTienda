import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import ClosedSessionHistory from "./ClosedSessionHistory";

vi.mock("../../shared/tauri/commands", () => ({
  annulSale: vi.fn(),
  getClosedSessionDetail: vi.fn(),
  listClosedSessions: vi.fn(),
}));

import {
  getClosedSessionDetail,
  listClosedSessions,
} from "../../shared/tauri/commands";

const mockGetClosedSessionDetail = vi.mocked(getClosedSessionDetail);
const mockListClosedSessions = vi.mocked(listClosedSessions);

const summaries = [
  {
    sessionId: "session-new",
    openedAt: "2026-08-03T09:00:00Z",
    closedAt: "2026-08-03T18:00:00Z",
    openingAmount: "100.00",
    expectedCash: "120.00",
    countedCash: "121.00",
    difference: "1.00",
  },
  {
    sessionId: "session-old",
    openedAt: "2026-08-02T09:00:00Z",
    closedAt: "2026-08-02T18:00:00Z",
    openingAmount: "80.00",
    expectedCash: "90.00",
    countedCash: "88.00",
    difference: "-2.00",
  },
  {
    sessionId: "session-even",
    openedAt: "2026-08-01T09:00:00Z",
    closedAt: "2026-08-01T18:00:00Z",
    openingAmount: "50.00",
    expectedCash: "50.00",
    countedCash: "50.00",
    difference: "0.00",
  },
] as const;

const detail = {
  ...summaries[0],
  operations: [
    {
      id: "sale-1",
      occurredAt: "2026-08-03T10:00:00Z",
      operationType: "sale" as const,
      amount: "100.00",
      detail: "Campera y cuaderno",
      category: "mixto" as const,
      payments: [
        {
          method: "transfer" as const,
          amount: "20.00",
          recipientAccount: "Carlos",
        },
      ],
      status: "annulled" as const,
      annulmentReason: "error de carga",
    },
    {
      id: "movement-1",
      occurredAt: "2026-08-03T11:00:00Z",
      operationType: "cash_out" as const,
      amount: "5.00",
      detail: "Compra de insumos",
      payments: [],
    },
  ],
  paymentTotals: {
    cash: "0.00",
    transfer: "20.00",
    debit: "0.00",
    credit: "0.00",
    qr: "0.00",
  },
  transferTotals: [{ recipientAccount: "Carlos", amount: "20.00" }],
  expectedCash: "120.00",
};

describe("ClosedSessionHistory", () => {
  beforeEach(() => {
    vi.resetAllMocks();
  });

  it("shows an empty state when no closed sessions exist", async () => {
    mockListClosedSessions.mockResolvedValue([]);

    render(<ClosedSessionHistory onBack={vi.fn()} />);

    expect(
      await screen.findByText("Todavía no hay cajas cerradas"),
    ).toBeInTheDocument();
    expect(mockGetClosedSessionDetail).not.toHaveBeenCalled();
  });

  it("shows reverse-chronological sessions and distinguishes differences", async () => {
    mockListClosedSessions.mockResolvedValue([...summaries]);
    mockGetClosedSessionDetail.mockResolvedValue(detail);

    render(<ClosedSessionHistory onBack={vi.fn()} />);

    const sessionList = await screen.findByRole("list", {
      name: "Sesiones cerradas",
    });
    const sessionButtons = within(sessionList).getAllByRole("button");
    expect(sessionButtons).toHaveLength(3);
    expect(sessionButtons[0]).toHaveTextContent("Sobrante: +$ 1,00");
    expect(sessionButtons[1]).toHaveTextContent("Faltante: -$ 2,00");
    expect(sessionButtons[2]).toHaveTextContent("Sin diferencia: $ 0,00");
    expect(sessionButtons[0]).toHaveTextContent("Efectivo esperado");
    expect(mockGetClosedSessionDetail).toHaveBeenCalledWith("session-new");
  });

  it("shows read-only detail and retries a failed detail query", async () => {
    const user = userEvent.setup();
    mockListClosedSessions.mockResolvedValue([...summaries]);
    mockGetClosedSessionDetail
      .mockRejectedValueOnce("DATABASE")
      .mockResolvedValueOnce(detail);

    render(<ClosedSessionHistory onBack={vi.fn()} />);

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "No se pudo consultar el detalle de la caja. Inténtalo de nuevo.",
    );
    await user.click(
      screen.getByRole("button", { name: "Reintentar detalle" }),
    );

    expect(
      await screen.findByRole("heading", { name: /Cierre del/ }),
    ).toBeInTheDocument();
    expect(screen.getByText("Campera y cuaderno")).toBeInTheDocument();
    expect(screen.getByText("Anulada")).toBeInTheDocument();
    expect(screen.getByText("Destino: Carlos")).toBeInTheDocument();
    expect(screen.getByText("Compra de insumos")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Anular venta" }),
    ).not.toBeInTheDocument();
    expect(screen.getByText("Resultado final")).toBeInTheDocument();
    expect(mockGetClosedSessionDetail).toHaveBeenCalledTimes(2);
  });

  it("retries a failed list query", async () => {
    const user = userEvent.setup();
    mockListClosedSessions
      .mockRejectedValueOnce("DATABASE")
      .mockResolvedValueOnce([]);

    render(<ClosedSessionHistory onBack={vi.fn()} />);

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "No se pudo consultar el historial de cajas. Inténtalo de nuevo.",
    );
    await user.click(
      screen.getByRole("button", { name: "Reintentar consulta" }),
    );
    expect(
      await screen.findByText("Todavía no hay cajas cerradas"),
    ).toBeInTheDocument();
    expect(mockListClosedSessions).toHaveBeenCalledTimes(2);
  });

  it("loads the selected session detail", async () => {
    const user = userEvent.setup();
    mockListClosedSessions.mockResolvedValue([...summaries]);
    mockGetClosedSessionDetail.mockImplementation(async (sessionId) =>
      sessionId === "session-old"
        ? {
            ...detail,
            ...summaries[1],
            operations: [
              {
                ...detail.operations[0],
                detail: "Operación de la sesión anterior",
              },
            ],
          }
        : detail,
    );

    render(<ClosedSessionHistory onBack={vi.fn()} />);

    const sessionList = await screen.findByRole("list", {
      name: "Sesiones cerradas",
    });
    const sessionButtons = within(sessionList).getAllByRole("button");
    expect(sessionButtons[0]).toHaveAttribute("aria-current", "true");
    expect(sessionButtons[1]).not.toHaveAttribute("aria-current");

    await user.click(sessionButtons[1]);

    await waitFor(() =>
      expect(mockGetClosedSessionDetail).toHaveBeenLastCalledWith(
        "session-old",
      ),
    );
    expect(sessionButtons[1]).toHaveAttribute("aria-current", "true");
    expect(sessionButtons[0]).not.toHaveAttribute("aria-current");
    expect(
      await screen.findByText("Operación de la sesión anterior"),
    ).toBeInTheDocument();
  });
});
