import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { ReactiveOverlay, type ReactivePayload } from "./ReactiveOverlay";

// 桩掉 @tauri-apps/api/core 的 invoke，这样组件不会真的 IPC
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

function makePayload(overrides: Partial<ReactivePayload> = {}): ReactivePayload {
  return {
    source: "clipboard",
    tier: "hint",
    icon: "format",
    preview: "Hello  world",
    charLen: 12,
    receivedAt: Date.now(),
    ...overrides,
  };
}

describe("ReactiveOverlay", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("not rendered when payload is null", () => {
    render(<ReactiveOverlay payload={null} lang="zh" onDismiss={() => {}} />);
    expect(screen.queryByTestId("rx-ribbon")).toBeNull();
  });

  it("not rendered when tier is acknowledge", () => {
    render(
      <ReactiveOverlay
        payload={makePayload({ tier: "acknowledge", icon: undefined })}
        lang="zh" onDismiss={() => {}} />
    );
    expect(screen.queryByTestId("rx-ribbon")).toBeNull();
  });

  it("renders 4 action pills for hint tier", () => {
    render(<ReactiveOverlay payload={makePayload()} lang="zh" onDismiss={() => {}} />);
    expect(screen.getByTestId("rx-action-clean")).toBeInTheDocument();
    expect(screen.getByTestId("rx-action-translate")).toBeInTheDocument();
    expect(screen.getByTestId("rx-action-explain")).toBeInTheDocument();
    expect(screen.getByTestId("rx-action-reply")).toBeInTheDocument();
  });

  it("shows source tag glyph based on source field", () => {
    const { rerender } = render(
      <ReactiveOverlay payload={makePayload({ source: "clipboard" })} lang="zh" onDismiss={() => {}} />
    );
    expect(screen.getByTestId("rx-ribbon")).toHaveAttribute("data-source", "clipboard");

    rerender(
      <ReactiveOverlay payload={makePayload({ source: "selection" })} lang="zh" onDismiss={() => {}} />
    );
    expect(screen.getByTestId("rx-ribbon")).toHaveAttribute("data-source", "selection");
  });

  it("clicking clean invokes process_reactive_action with action=clean", async () => {
    invokeMock.mockResolvedValueOnce("cleaned text");
    render(<ReactiveOverlay payload={makePayload()} lang="zh" onDismiss={() => {}} />);
    await userEvent.click(screen.getByTestId("rx-action-clean"));
    expect(invokeMock).toHaveBeenCalledWith("process_reactive_action", { action: "clean" });
  });

  it.each([
    ["translate", "rx-action-translate"],
    ["explain", "rx-action-explain"],
    ["reply", "rx-action-reply"],
  ] as const)("clicking %s invokes with matching action", async (action, testId) => {
    invokeMock.mockResolvedValueOnce("ok");
    render(
      <ReactiveOverlay payload={makePayload({ receivedAt: Math.random() })}
        lang="zh" onDismiss={() => {}} />
    );
    await userEvent.click(screen.getByTestId(testId));
    expect(invokeMock).toHaveBeenCalledWith("process_reactive_action", { action });
  });

  it("transitions idle → busy → done on successful action", async () => {
    let resolveInvoke!: (v: string) => void;
    invokeMock.mockImplementationOnce(
      () => new Promise<string>((r) => { resolveInvoke = r; })
    );
    render(<ReactiveOverlay payload={makePayload()} lang="zh" onDismiss={() => {}} />);
    await userEvent.click(screen.getByTestId("rx-action-clean"));
    // busy state
    expect(screen.getByTestId("rx-status-busy")).toBeInTheDocument();
    expect(screen.queryByTestId("rx-action-clean")).toBeNull();
    // resolve
    resolveInvoke("the cleaned result");
    await waitFor(() => expect(screen.getByTestId("rx-status-done")).toBeInTheDocument());
  });

  it("transitions idle → busy → err on rejected action", async () => {
    invokeMock.mockRejectedValueOnce("simulated failure");
    render(<ReactiveOverlay payload={makePayload()} lang="zh" onDismiss={() => {}} />);
    await userEvent.click(screen.getByTestId("rx-action-translate"));
    await waitFor(() => expect(screen.getByTestId("rx-status-err")).toBeInTheDocument());
    expect(screen.getByTestId("rx-status-err").textContent).toContain("simulated failure");
  });

  it("auto-dismisses after AUTO_DISMISS_MS (idle phase)", async () => {
    vi.useFakeTimers();
    const onDismiss = vi.fn();
    render(<ReactiveOverlay payload={makePayload()} lang="zh" onDismiss={onDismiss} />);
    vi.advanceTimersByTime(3999);
    expect(onDismiss).not.toHaveBeenCalled();
    vi.advanceTimersByTime(2);
    expect(onDismiss).toHaveBeenCalled();
    vi.useRealTimers();
  });

  it("Esc closes immediately", async () => {
    const onDismiss = vi.fn();
    render(<ReactiveOverlay payload={makePayload()} lang="zh" onDismiss={onDismiss} />);
    await userEvent.keyboard("{Escape}");
    expect(onDismiss).toHaveBeenCalled();
  });

  it("renders English strings when lang=en", () => {
    render(<ReactiveOverlay payload={makePayload()} lang="en" onDismiss={() => {}} />);
    expect(screen.getByTestId("rx-action-clean").textContent).toContain("Plain");
    expect(screen.getByTestId("rx-action-translate").textContent).toContain("Translate");
    expect(screen.getByTestId("rx-action-reply").textContent).toContain("Reply");
  });

  it("payload swap resets phase to idle", async () => {
    invokeMock.mockResolvedValueOnce("ok");
    const { rerender } = render(<ReactiveOverlay payload={makePayload({ receivedAt: 1 })} lang="zh" onDismiss={() => {}} />);
    await userEvent.click(screen.getByTestId("rx-action-clean"));
    await waitFor(() => expect(screen.getByTestId("rx-status-done")).toBeInTheDocument());
    rerender(<ReactiveOverlay payload={makePayload({ receivedAt: 2 })} lang="zh" onDismiss={() => {}} />);
    // 新 payload → phase 重置回 idle
    expect(screen.getByTestId("rx-action-clean")).toBeInTheDocument();
    expect(screen.queryByTestId("rx-status-done")).toBeNull();
  });
});
