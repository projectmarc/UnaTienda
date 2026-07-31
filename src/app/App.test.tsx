import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import App from "./App";

vi.mock("@/features/sessions/SessionView", () => ({
  default: () => <div>Flujo de sesión conectado</div>,
}));

describe("App", () => {
  it("muestra el flujo de sesión dentro de la aplicación visible", () => {
    render(<App />);

    expect(
      screen.getByRole("heading", { name: "UnaTienda" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Flujo de sesión conectado")).toBeInTheDocument();
    expect(
      screen.queryByText("Base de interfaz lista"),
    ).not.toBeInTheDocument();
  });
});
