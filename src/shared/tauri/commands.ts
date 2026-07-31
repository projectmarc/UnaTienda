import { invoke } from "@tauri-apps/api/core";

// --- Cash Session Types ---

export interface OpenSessionResponse {
  sessionId: string;
  openingAmount: string;
}

export interface ClosedSessionResponse {
  sessionId: string;
  openingAmount: string;
  expectedCash: string;
  countedCash: string;
  difference: string;
}

export interface ReconciliationView {
  sessionId: string;
  openingAmount: string;
  expectedCash: string;
  countedCash?: string;
  difference?: string;
}

// --- Cash Session Commands ---

export async function openSession(
  openingAmount: string,
  idempotencyKey: string,
): Promise<OpenSessionResponse> {
  return invoke<OpenSessionResponse>("open_session", {
    openingAmount,
    idempotencyKey,
  });
}

export async function closeSession(
  countedCash: string,
  idempotencyKey: string,
): Promise<ClosedSessionResponse> {
  return invoke<ClosedSessionResponse>("close_session", {
    countedCash,
    idempotencyKey,
  });
}

export async function getReconciliation(
  countedCash?: string,
): Promise<ReconciliationView> {
  return invoke<ReconciliationView>("get_reconciliation", {
    countedCash: countedCash ?? null,
  });
}
