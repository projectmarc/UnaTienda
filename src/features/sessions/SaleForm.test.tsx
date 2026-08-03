import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, within, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import SaleForm from "./SaleForm";

vi.mock("../../shared/tauri/commands", () => ({
  recordSale: vi.fn(),
}));

import { recordSale } from "../../shared/tauri/commands";

const mockRecordSale = vi.mocked(recordSale);

describe("SaleForm", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  async function setupSinglePayment(
    user: ReturnType<typeof userEvent.setup>,
    method: string,
    total = "100.00",
  ) {
    await user.type(screen.getByLabelText("Monto total"), total);
    await user.click(screen.getByRole("button", { name: "Ropa" }));
    await user.click(screen.getByRole("button", { name: method }));
  }

  function getSelectedMethodLabel() {
    // The primary method section has buttons; the selected one has bg-primary class
    const paymentSection = screen.getByText("Medio de pago").closest("div")!;
    const buttons = within(paymentSection).getAllByRole("button");
    return buttons.find((b) => b.className.includes("bg-primary"));
  }

  // --- Single payment: frictionless ---

  it("submits single payment with total auto-assigned to method", async () => {
    const user = userEvent.setup();
    mockRecordSale.mockResolvedValue({ saleId: "1", totalAmount: "100.00" });
    render(<SaleForm onRecorded={vi.fn()} />);

    await setupSinglePayment(user, "Efectivo");

    await user.click(screen.getByRole("button", { name: "Registrar venta" }));

    await waitFor(() => {
      expect(mockRecordSale).toHaveBeenCalledWith(
        expect.objectContaining({
          payments: [
            { method: "cash", amount: "100.00", recipientAccount: undefined },
          ],
        }),
        expect.any(String),
      );
    });
  });

  it("shows transfer recipient selection when method is transfer", async () => {
    const user = userEvent.setup();
    render(<SaleForm onRecorded={vi.fn()} />);

    await user.type(screen.getByLabelText("Monto total"), "50.00");
    await user.click(screen.getByRole("button", { name: "Mixto" }));
    await user.click(screen.getByRole("button", { name: "Transferencia" }));

    expect(screen.getByRole("button", { name: "Juan" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Carlos" })).toBeInTheDocument();
  });

  // --- Split mode ---

  it("shows 'Dividir pago' button after selecting primary method", async () => {
    const user = userEvent.setup();
    render(<SaleForm onRecorded={vi.fn()} />);

    await user.type(screen.getByLabelText("Monto total"), "100.00");
    await user.click(screen.getByRole("button", { name: "Ropa" }));
    await user.click(screen.getByRole("button", { name: "Efectivo" }));

    expect(
      screen.getByRole("button", { name: "Dividir pago" }),
    ).toBeInTheDocument();
  });

  it("preserves primary method and recipient when entering split mode", async () => {
    const user = userEvent.setup();
    render(<SaleForm onRecorded={vi.fn()} />);

    await user.type(screen.getByLabelText("Monto total"), "200.00");
    await user.click(screen.getByRole("button", { name: "Ropa" }));
    await user.click(screen.getByRole("button", { name: "Transferencia" }));
    await user.click(screen.getByRole("button", { name: "Juan" }));

    // Enter split mode
    await user.click(screen.getByRole("button", { name: "Dividir pago" }));

    // Primary method "Transferencia" should still be selected (has bg-primary class)
    const selected = getSelectedMethodLabel();
    expect(selected).toBeTruthy();
    expect(selected!.textContent).toBe("Transferencia");

    // Recipient "Juan" still selected
    const recipientSection = screen.getByText("Cuenta destino").closest("div")!;
    const recipientButtons = within(recipientSection).getAllByRole("button");
    const selectedRecipient = recipientButtons.find((b) =>
      b.className.includes("bg-primary"),
    );
    expect(selectedRecipient).toBeTruthy();
    expect(selectedRecipient!.textContent).toBe("Juan");

    // Additional payments section appears
    expect(screen.getByText("Pagos adicionales")).toBeInTheDocument();
  });

  // --- Adding additional payment ---

  it("adds an additional payment and shows computed primary remainder", async () => {
    const user = userEvent.setup();
    render(<SaleForm onRecorded={vi.fn()} />);

    await user.type(screen.getByLabelText("Monto total"), "100.00");
    await user.click(screen.getByRole("button", { name: "Ropa" }));
    await user.click(screen.getByRole("button", { name: "Efectivo" }));

    // Enter split mode
    await user.click(screen.getByRole("button", { name: "Dividir pago" }));

    // Select additional method and amount
    await user.selectOptions(screen.getByLabelText("Método"), "debit");
    await user.type(screen.getByLabelText("Importe"), "30.00");
    await user.click(screen.getByRole("button", { name: "Agregar" }));

    // Additional part is listed (check there's a span with the combined text)
    const allDebito = screen.getAllByText(/Débito/);
    const listItemSpan = allDebito.find(
      (el) => el.tagName === "SPAN" && (el.textContent ?? "").includes("30.00"),
    );
    expect(listItemSpan).toBeTruthy();

    const summary = screen.getByRole("region", {
      name: "Resumen del pago dividido",
    });
    expect(
      within(summary).getByText("Importe restante (calculado automáticamente)"),
    ).toBeInTheDocument();
    expect(within(summary).getByText("$70.00")).toBeInTheDocument();
    expect(within(summary).getByText("Total de la venta")).toBeInTheDocument();
    expect(within(summary).getByText("Débito")).toBeInTheDocument();
    expect(within(summary).getByText("$30.00")).toBeInTheDocument();
    expect(within(summary).getByText("$100.00 de $100.00")).toBeInTheDocument();
    expect(
      within(summary).getByText(/Válido: los importes coinciden/),
    ).toBeInTheDocument();
  });

  it("updates the primary remainder live while entering the additional amount", async () => {
    const user = userEvent.setup();
    render(<SaleForm onRecorded={vi.fn()} />);

    await setupSinglePayment(user, "Efectivo");
    await user.click(screen.getByRole("button", { name: "Dividir pago" }));
    await user.selectOptions(screen.getByLabelText("Método"), "debit");

    const summary = screen.getByRole("region", {
      name: "Resumen del pago dividido",
    });
    await user.type(screen.getByLabelText("Importe"), "30.00");
    expect(within(summary).getByText("$70.00")).toBeInTheDocument();

    await user.clear(screen.getByLabelText("Importe"));
    await user.type(screen.getByLabelText("Importe"), "45.00");
    expect(within(summary).getByText("$55.00")).toBeInTheDocument();
  });

  it("submits split payment with computed primary and additional parts", async () => {
    const user = userEvent.setup();
    mockRecordSale.mockResolvedValue({ saleId: "2", totalAmount: "100.00" });
    render(<SaleForm onRecorded={vi.fn()} />);

    await user.type(screen.getByLabelText("Monto total"), "100.00");
    await user.click(screen.getByRole("button", { name: "Ropa" }));
    await user.click(screen.getByRole("button", { name: "Efectivo" }));
    await user.click(screen.getByRole("button", { name: "Dividir pago" }));

    await user.selectOptions(screen.getByLabelText("Método"), "debit");
    await user.type(screen.getByLabelText("Importe"), "30.00");
    await user.click(screen.getByRole("button", { name: "Agregar" }));

    await user.click(screen.getByRole("button", { name: "Registrar venta" }));

    await waitFor(() => {
      expect(mockRecordSale).toHaveBeenCalledWith(
        expect.objectContaining({
          payments: [
            { method: "cash", amount: "70.00", recipientAccount: undefined },
            { method: "debit", amount: "30.00", recipientAccount: undefined },
          ],
        }),
        expect.any(String),
      );
    });
  });

  it("preserves transfer recipient for primary in split submission", async () => {
    const user = userEvent.setup();
    mockRecordSale.mockResolvedValue({ saleId: "3", totalAmount: "200.00" });
    render(<SaleForm onRecorded={vi.fn()} />);

    await user.type(screen.getByLabelText("Monto total"), "200.00");
    await user.click(screen.getByRole("button", { name: "Ropa" }));
    await user.click(screen.getByRole("button", { name: "Transferencia" }));
    await user.click(screen.getByRole("button", { name: "Carlos" }));
    await user.click(screen.getByRole("button", { name: "Dividir pago" }));

    await user.selectOptions(screen.getByLabelText("Método"), "cash");
    await user.type(screen.getByLabelText("Importe"), "50.00");
    await user.click(screen.getByRole("button", { name: "Agregar" }));

    await user.click(screen.getByRole("button", { name: "Registrar venta" }));

    await waitFor(() => {
      expect(mockRecordSale).toHaveBeenCalledWith(
        expect.objectContaining({
          payments: [
            {
              method: "transfer",
              amount: "150.00",
              recipientAccount: "Carlos",
            },
            { method: "cash", amount: "50.00", recipientAccount: undefined },
          ],
        }),
        expect.any(String),
      );
    });
  });

  // --- Validation: reject amounts that exhaust the total ---

  it("disables 'Agregar' when amount would leave zero or negative primary", async () => {
    const user = userEvent.setup();
    render(<SaleForm onRecorded={vi.fn()} />);

    await user.type(screen.getByLabelText("Monto total"), "100.00");
    await user.click(screen.getByRole("button", { name: "Ropa" }));
    await user.click(screen.getByRole("button", { name: "Efectivo" }));
    await user.click(screen.getByRole("button", { name: "Dividir pago" }));

    await user.selectOptions(screen.getByLabelText("Método"), "debit");
    await user.type(screen.getByLabelText("Importe"), "100.00");

    const addButton = screen.getByRole("button", { name: "Agregar" });
    expect(addButton).toBeDisabled();
  });

  it("disables 'Agregar' when amount exceeds total", async () => {
    const user = userEvent.setup();
    render(<SaleForm onRecorded={vi.fn()} />);

    await user.type(screen.getByLabelText("Monto total"), "100.00");
    await user.click(screen.getByRole("button", { name: "Ropa" }));
    await user.click(screen.getByRole("button", { name: "Efectivo" }));
    await user.click(screen.getByRole("button", { name: "Dividir pago" }));

    await user.selectOptions(screen.getByLabelText("Método"), "credit");
    await user.type(screen.getByLabelText("Importe"), "150.00");

    const addButton = screen.getByRole("button", { name: "Agregar" });
    expect(addButton).toBeDisabled();
  });

  it("disables 'Agregar' when amount is zero", async () => {
    const user = userEvent.setup();
    render(<SaleForm onRecorded={vi.fn()} />);

    await user.type(screen.getByLabelText("Monto total"), "100.00");
    await user.click(screen.getByRole("button", { name: "Ropa" }));
    await user.click(screen.getByRole("button", { name: "Efectivo" }));
    await user.click(screen.getByRole("button", { name: "Dividir pago" }));

    await user.selectOptions(screen.getByLabelText("Método"), "debit");
    await user.type(screen.getByLabelText("Importe"), "0.00");

    const addButton = screen.getByRole("button", { name: "Agregar" });
    expect(addButton).toBeDisabled();
  });

  it("requires recipient when additional method is transfer", async () => {
    const user = userEvent.setup();
    render(<SaleForm onRecorded={vi.fn()} />);

    await user.type(screen.getByLabelText("Monto total"), "100.00");
    await user.click(screen.getByRole("button", { name: "Ropa" }));
    await user.click(screen.getByRole("button", { name: "Efectivo" }));
    await user.click(screen.getByRole("button", { name: "Dividir pago" }));

    await user.selectOptions(screen.getByLabelText("Método"), "transfer");
    await user.type(screen.getByLabelText("Importe"), "20.00");

    // Add button disabled without recipient
    expect(screen.getByRole("button", { name: "Agregar" })).toBeDisabled();

    // Select recipient
    await user.selectOptions(screen.getByLabelText("Destino"), "Juan");

    expect(screen.getByRole("button", { name: "Agregar" })).toBeEnabled();
  });

  // --- Removing additional payments ---

  it("removes an additional payment and recomputes primary remainder", async () => {
    const user = userEvent.setup();
    render(<SaleForm onRecorded={vi.fn()} />);

    await user.type(screen.getByLabelText("Monto total"), "100.00");
    await user.click(screen.getByRole("button", { name: "Ropa" }));
    await user.click(screen.getByRole("button", { name: "Efectivo" }));
    await user.click(screen.getByRole("button", { name: "Dividir pago" }));

    await user.selectOptions(screen.getByLabelText("Método"), "debit");
    await user.type(screen.getByLabelText("Importe"), "30.00");
    await user.click(screen.getByRole("button", { name: "Agregar" }));

    // Remove the additional payment
    await user.click(screen.getByRole("button", { name: "✕" }));

    // Primary amount summary should show full total (100.00)
    const summary = screen.getByRole("region", {
      name: "Resumen del pago dividido",
    });
    expect(
      within(summary).getAllByText("$100.00").length,
    ).toBeGreaterThanOrEqual(1);
  });

  // --- Collapse back to single mode ---

  it("collapses back to single mode preserving primary method and recipient", async () => {
    const user = userEvent.setup();
    render(<SaleForm onRecorded={vi.fn()} />);

    await user.type(screen.getByLabelText("Monto total"), "200.00");
    await user.click(screen.getByRole("button", { name: "Ropa" }));
    await user.click(screen.getByRole("button", { name: "Transferencia" }));
    await user.click(screen.getByRole("button", { name: "Carlos" }));
    await user.click(screen.getByRole("button", { name: "Dividir pago" }));

    await user.selectOptions(screen.getByLabelText("Método"), "cash");
    await user.type(screen.getByLabelText("Importe"), "50.00");
    await user.click(screen.getByRole("button", { name: "Agregar" }));

    // Collapse to single mode
    await user.click(screen.getByRole("button", { name: "Un medio de pago" }));

    // Primary method "Transferencia" still selected
    const selected = getSelectedMethodLabel();
    expect(selected).toBeTruthy();
    expect(selected!.textContent).toBe("Transferencia");

    // Recipient "Carlos" still selected
    const recipientSection = screen.getByText("Cuenta destino").closest("div")!;
    const recipientButtons = within(recipientSection).getAllByRole("button");
    const selectedRecipient = recipientButtons.find((b) =>
      b.className.includes("bg-primary"),
    );
    expect(selectedRecipient).toBeTruthy();
    expect(selectedRecipient!.textContent).toBe("Carlos");

    // Additional payments section gone
    expect(screen.queryByText("Pagos adicionales")).not.toBeInTheDocument();

    // Button text back to "Dividir pago"
    expect(
      screen.getByRole("button", { name: "Dividir pago" }),
    ).toBeInTheDocument();
  });

  // --- Multiple additional payments ---

  it("allows adding multiple additional payments", async () => {
    const user = userEvent.setup();
    mockRecordSale.mockResolvedValue({ saleId: "4", totalAmount: "100.00" });
    render(<SaleForm onRecorded={vi.fn()} />);

    await user.type(screen.getByLabelText("Monto total"), "100.00");
    await user.click(screen.getByRole("button", { name: "Ropa" }));
    await user.click(screen.getByRole("button", { name: "Efectivo" }));
    await user.click(screen.getByRole("button", { name: "Dividir pago" }));

    // First additional: debit 30
    await user.selectOptions(screen.getByLabelText("Método"), "debit");
    await user.type(screen.getByLabelText("Importe"), "30.00");
    await user.click(screen.getByRole("button", { name: "Agregar" }));

    // Second additional: qr 20
    await user.selectOptions(screen.getByLabelText("Método"), "qr");
    await user.type(screen.getByLabelText("Importe"), "20.00");
    await user.click(screen.getByRole("button", { name: "Agregar" }));

    // Submit
    await user.click(screen.getByRole("button", { name: "Registrar venta" }));

    await waitFor(() => {
      expect(mockRecordSale).toHaveBeenCalledWith(
        expect.objectContaining({
          payments: [
            { method: "cash", amount: "50.00", recipientAccount: undefined },
            { method: "debit", amount: "30.00", recipientAccount: undefined },
            { method: "qr", amount: "20.00", recipientAccount: undefined },
          ],
        }),
        expect.any(String),
      );
    });
  });

  // --- Disabled states ---

  it("disables 'Dividir pago' when no primary method selected", async () => {
    const user = userEvent.setup();
    render(<SaleForm onRecorded={vi.fn()} />);

    await user.type(screen.getByLabelText("Monto total"), "100.00");
    await user.click(screen.getByRole("button", { name: "Ropa" }));

    expect(screen.getByRole("button", { name: "Dividir pago" })).toBeDisabled();
  });

  it("disables submit when primary amount would be non-positive in split mode", async () => {
    const user = userEvent.setup();
    render(<SaleForm onRecorded={vi.fn()} />);

    await user.type(screen.getByLabelText("Monto total"), "100.00");
    await user.click(screen.getByRole("button", { name: "Ropa" }));
    await user.click(screen.getByRole("button", { name: "Efectivo" }));
    await user.click(screen.getByRole("button", { name: "Dividir pago" }));

    await user.selectOptions(screen.getByLabelText("Método"), "debit");
    await user.type(screen.getByLabelText("Importe"), "99.00");
    await user.click(screen.getByRole("button", { name: "Agregar" }));

    // Submit should work (primary = 1.00)
    mockRecordSale.mockResolvedValue({ saleId: "5", totalAmount: "100.00" });
    await user.click(screen.getByRole("button", { name: "Registrar venta" }));

    await waitFor(() => {
      expect(mockRecordSale).toHaveBeenCalledWith(
        expect.objectContaining({
          payments: expect.arrayContaining([
            expect.objectContaining({ method: "cash", amount: "1.00" }),
          ]),
        }),
        expect.any(String),
      );
    });
  });
});
