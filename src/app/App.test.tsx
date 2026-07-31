import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import App from "./App";

describe("App scaffold", () => {
  it("renders the foundation placeholder", () => {
    render(<App />);
    expect(screen.getByText("UnaTienda POS")).toBeInTheDocument();
    expect(screen.getByText("Foundation scaffold loaded.")).toBeInTheDocument();
  });
});
