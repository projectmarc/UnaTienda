import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "./App";

vi.mock("@/features/sessions/SessionView", () => ({
  default: () => <div>Flujo de sesión conectado</div>,
}));

vi.mock("@/features/sessions/ClosedSessionHistory", () => ({
  default: () => <div>Historial conectado</div>,
}));

vi.mock("@/features/backups/BackupPanel", () => ({
  default: () => <div>Copias conectadas</div>,
}));

describe("App", () => {
  it("muestra el flujo de sesión dentro de la aplicación visible", () => {
    render(<App />);

    expect(
      screen.getByRole("heading", { name: "UnaTienda" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Historial" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Flujo de sesión conectado")).toBeInTheDocument();
    expect(
      screen.queryByText("Base de interfaz lista"),
    ).not.toBeInTheDocument();
  });

  it("mueve el foco al contenido y oculta el progreso fuera de Operación", async () => {
    const user = userEvent.setup();
    render(<App />);

    expect(screen.getByText("Progreso de sesión")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Historial" }));

    expect(screen.queryByText("Progreso de sesión")).not.toBeInTheDocument();
    expect(screen.getByRole("main")).toHaveFocus();

    await user.click(screen.getByRole("button", { name: "Copias" }));
    expect(screen.queryByText("Progreso de sesión")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Operación" }));
    expect(screen.getByText("Progreso de sesión")).toBeInTheDocument();
  });
});
