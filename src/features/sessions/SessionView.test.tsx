import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import SessionView from "./SessionView";

// Mock the Tauri command adapter.
vi.mock("../../shared/tauri/commands", () => ({
  openSession: vi.fn(),
  getReconciliation: vi.fn(),
  closeSession: vi.fn(),
}));

import {
  getReconciliation,
  closeSession,
  openSession,
} from "../../shared/tauri/commands";

const mockOpenSession = vi.mocked(openSession);
const mockGetReconciliation = vi.mocked(getReconciliation);
const mockCloseSession = vi.mocked(closeSession);

describe("SessionView", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("displays loading state initially", () => {
    mockGetReconciliation.mockReturnValue(new Promise(() => {})); // never resolves
    render(<SessionView />);
    expect(
      screen.getByText("Consultando la sesión de caja…"),
    ).toBeInTheDocument();
  });

  it("displays reconciliation data on load", async () => {
    mockGetReconciliation.mockResolvedValue({
      sessionId: "test-session",
      openingAmount: "100.00",
      expectedCash: "100.00",
    });

    render(<SessionView />);

    await waitFor(() => {
      expect(screen.getByText("Open Cash Session")).toBeInTheDocument();
    });

    // Both opening and expected show 100.00; verify the structure.
    const amounts = screen.getAllByText("100.00");
    expect(amounts).toHaveLength(2);
    expect(screen.getByText("Opening Amount")).toBeInTheDocument();
    expect(screen.getByText("Expected Cash")).toBeInTheDocument();
  });

  it("displays the opening form when no session exists", async () => {
    mockGetReconciliation.mockRejectedValue("NO_OPEN_SESSION");

    render(<SessionView />);

    await waitFor(() => {
      expect(screen.getByLabelText("Efectivo inicial")).toBeInTheDocument();
    });
  });

  it("opens a session and displays its reconciliation", async () => {
    const user = userEvent.setup();
    mockGetReconciliation
      .mockRejectedValueOnce("NO_OPEN_SESSION")
      .mockResolvedValueOnce({
        sessionId: "test-session",
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
    expect(await screen.findByText("Open Cash Session")).toBeInTheDocument();
  });

  it("reuses the opening key when retrying a recoverable error", async () => {
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

  it("localizes reconciliation query failures", async () => {
    mockGetReconciliation.mockRejectedValue("CONNECTION_LOST");

    render(<SessionView />);

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "No se pudo consultar la sesión de caja.",
    );
  });

  it("calls getReconciliation with counted cash for exact difference", async () => {
    const user = userEvent.setup();

    // Initial load without counted cash.
    mockGetReconciliation.mockResolvedValueOnce({
      sessionId: "test-session",
      openingAmount: "100.00",
      expectedCash: "100.00",
    });

    // Second call with counted cash returns the exact difference.
    mockGetReconciliation.mockResolvedValueOnce({
      sessionId: "test-session",
      openingAmount: "100.00",
      expectedCash: "100.00",
      countedCash: "105.00",
      difference: "5.00",
    });

    render(<SessionView />);

    await waitFor(() => {
      expect(screen.getByText("Open Cash Session")).toBeInTheDocument();
    });

    const input = screen.getByLabelText("Counted Cash");
    await user.type(input, "105.00");

    // Wait for debounced call (300ms debounce + buffer).
    await waitFor(
      () => {
        expect(mockGetReconciliation).toHaveBeenCalledWith("105.00");
      },
      { timeout: 1000 },
    );

    // The exact difference from the backend should be displayed.
    await waitFor(() => {
      expect(screen.getByText("Difference: 5.00")).toBeInTheDocument();
    });
  });

  it("displays negative difference from backend, not parseFloat", async () => {
    const user = userEvent.setup();

    mockGetReconciliation.mockResolvedValueOnce({
      sessionId: "test-session",
      openingAmount: "100.00",
      expectedCash: "100.00",
    });

    // Backend returns exact negative difference.
    mockGetReconciliation.mockResolvedValueOnce({
      sessionId: "test-session",
      openingAmount: "100.00",
      expectedCash: "100.00",
      countedCash: "90.00",
      difference: "-10.00",
    });

    render(<SessionView />);

    await waitFor(() => {
      expect(screen.getByText("Open Cash Session")).toBeInTheDocument();
    });

    const input = screen.getByLabelText("Counted Cash");
    await user.type(input, "90.00");

    await waitFor(
      () => {
        expect(mockGetReconciliation).toHaveBeenCalledWith("90.00");
      },
      { timeout: 1000 },
    );

    await waitFor(() => {
      expect(screen.getByText("Difference: -10.00")).toBeInTheDocument();
    });
  });

  it("does not display difference when counted cash is empty", async () => {
    mockGetReconciliation.mockResolvedValue({
      sessionId: "test-session",
      openingAmount: "100.00",
      expectedCash: "100.00",
    });

    render(<SessionView />);

    await waitFor(() => {
      expect(screen.getByText("Open Cash Session")).toBeInTheDocument();
    });

    // No difference should be displayed when countedCash is not set.
    expect(screen.queryByText(/Difference:/)).not.toBeInTheDocument();
  });

  it("displays closed result after successful close", async () => {
    const user = userEvent.setup();

    mockGetReconciliation.mockResolvedValue({
      sessionId: "test-session",
      openingAmount: "100.00",
      expectedCash: "100.00",
      countedCash: "105.00",
      difference: "5.00",
    });

    mockCloseSession.mockResolvedValue({
      sessionId: "test-session",
      openingAmount: "100.00",
      expectedCash: "100.00",
      countedCash: "105.00",
      difference: "5.00",
    });

    render(<SessionView />);

    await waitFor(() => {
      expect(screen.getByText("Open Cash Session")).toBeInTheDocument();
    });

    const input = screen.getByLabelText("Counted Cash");
    await user.type(input, "105.00");

    const button = screen.getByText("Close Session");
    await user.click(button);

    await waitFor(() => {
      expect(screen.getByText("Session Closed")).toBeInTheDocument();
    });

    // Verify the closed result shows the exact amounts.
    expect(screen.getByText("105.00")).toBeInTheDocument();
    expect(screen.getByText("5.00")).toBeInTheDocument();
  });

  it("displays error on close failure", async () => {
    const user = userEvent.setup();

    mockGetReconciliation.mockResolvedValue({
      sessionId: "test-session",
      openingAmount: "100.00",
      expectedCash: "100.00",
      countedCash: "105.00",
      difference: "5.00",
    });

    mockCloseSession.mockRejectedValue("DRAFTS_BLOCK_CLOSE");

    render(<SessionView />);

    await waitFor(() => {
      expect(screen.getByText("Open Cash Session")).toBeInTheDocument();
    });

    const input = screen.getByLabelText("Counted Cash");
    await user.type(input, "105.00");

    const button = screen.getByText("Close Session");
    await user.click(button);

    await waitFor(() => {
      expect(screen.getByRole("alert")).toHaveTextContent("DRAFTS_BLOCK_CLOSE");
    });
  });
});
