import { describe, expect, it, vi } from "vitest";
import { useEffect } from "react";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "./App";

type MockSessionViewProps = {
  onProgressChange?: (step: "operation") => void;
  requestedStep?: "reconcile" | null;
};

vi.mock("@/features/sessions/SessionView", () => ({
  default: function MockSessionView({
    onProgressChange,
    requestedStep,
  }: MockSessionViewProps) {
    useEffect(() => onProgressChange?.("operation"), [onProgressChange]);

    return (
      <>
        <div>Flujo de sesión conectado</div>
        {requestedStep === "reconcile" && <div>Solicitud de cierre</div>}
      </>
    );
  },
}));

vi.mock("@/features/sessions/ClosedSessionHistory", () => ({
  default: () => <div>Historial conectado</div>,
}));

vi.mock("@/features/backups/BackupPanel", () => ({
  default: () => <div>Copias conectadas</div>,
}));

describe("App", () => {
  it("muestra la jerarquía de navegación con sus etiquetas exactas", () => {
    render(<App />);

    expect(
      screen.getByRole("heading", { name: "UnaTienda" }),
    ).toBeInTheDocument();
    const sections = screen.getByRole("navigation", { name: "Secciones" });
    expect(
      within(within(sections).getByRole("group", { name: "Principal" }))
        .getAllByRole("button")
        .map((button) => button.textContent),
    ).toEqual(["Ventas", "Caja actual", "Historial"]);
    expect(
      within(within(sections).getByRole("group", { name: "Secundario" }))
        .getAllByRole("button")
        .map((button) => button.textContent),
    ).toEqual(["Movimientos", "Configuración"]);
    expect(
      screen.getByRole("button", { name: "Configuración" }),
    ).toBeDisabled();
    expect(screen.getByRole("button", { name: "Ventas" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(
      screen.getByText("Modo local · Los datos se guardan en este equipo"),
    ).toBeInTheDocument();
    expect(screen.getByText("Flujo de sesión conectado")).toBeInTheDocument();
  });

  it("mueve el foco al contenido y mantiene el progreso de sesión persistente", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(screen.getByRole("button", { name: "Historial" }));

    expect(screen.getByText("Progreso de sesión")).toBeInTheDocument();
    expect(screen.getByRole("main")).toHaveFocus();
    await user.click(
      screen.getByRole("button", { name: "Copias de seguridad" }),
    );
    expect(screen.getByText("Copias conectadas")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Ventas" }));
  });

  it("muestra estados transitorios honestos para las secciones futuras", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(screen.getByRole("button", { name: "Caja actual" }));
    expect(
      screen.getByRole("heading", { name: "Caja actual" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText("Disponible en la siguiente actualización."),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Movimientos" }));
    expect(screen.getByRole("button", { name: "Movimientos" })).toHaveAttribute(
      "aria-current",
      "page",
    );
  });

  it("permite llegar a Cierre desde el progreso de una sesión abierta", async () => {
    const user = userEvent.setup();
    render(<App />);

    const closeStep = screen.getByRole("button", { name: "Cierre" });

    await user.click(closeStep);

    expect(screen.getByText("Solicitud de cierre")).toBeInTheDocument();
  });
});
