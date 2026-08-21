import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { App } from "../src/App.tsx";

describe("webserver h5 shell", () => {
  it("renders the mobile console heading", () => {
    render(<App />);
    expect(screen.getByRole("heading", { name: "Mobile console" })).toBeTruthy();
  });
});
